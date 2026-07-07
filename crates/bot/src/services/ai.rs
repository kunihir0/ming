use anyhow::{Context, Result};
use moka::future::Cache;
use once_cell::sync::Lazy;
use reqwest::Client;
use rustplus::proto::AppMarker;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::utils::map::get_grid_pos;
use crate::utils::vending::get_item_name;

static AI_CACHE: Lazy<Cache<(i32, Option<String>), String>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(300)) // 5 minutes cache per server+query
        .build()
});

#[derive(Serialize)]
struct GeminiRequest {
    #[serde(rename = "systemInstruction", skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTool>>,
}

#[derive(Serialize, Deserialize, Clone)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: FunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: FunctionResponse,
    },
}

#[derive(Serialize, Deserialize, Clone)]
struct FunctionCall {
    name: String,
    args: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone)]
struct FunctionResponse {
    name: String,
    response: FunctionResponseData,
}

#[derive(Serialize, Deserialize, Clone)]
struct FunctionResponseData {
    content: String,
}

#[derive(Serialize, Clone)]
struct GeminiTool {
    #[serde(rename = "functionDeclarations")]
    function_declarations: Vec<FunctionDeclaration>,
}

#[derive(Serialize, Clone)]
struct FunctionDeclaration {
    name: String,
    description: String,
    parameters: Option<FunctionParameters>,
}

#[derive(Serialize, Clone)]
struct FunctionParameters {
    #[serde(rename = "type")]
    type_: String,
    properties: std::collections::HashMap<String, ParameterProperty>,
}

#[derive(Serialize, Clone)]
struct ParameterProperty {
    #[serde(rename = "type")]
    type_: String,
    description: String,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Option<Vec<Candidate>>,
    error: Option<GeminiError>,
}

#[derive(Deserialize)]
struct GeminiError {
    message: String,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<GeminiContent>,
}

pub async fn find_best_deals(
    server_id: i32,
    map_size: u32,
    vending_machines: &[AppMarker],
    query: Option<String>,
) -> Result<String> {
    let cache_key = (server_id, query.clone());
    if let Some(cached) = AI_CACHE.get(&cache_key).await {
        return Ok(cached);
    }

    let api_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
    if api_key.is_empty() || api_key == "your_gemini_api_key_here" {
        anyhow::bail!("GEMINI_API_KEY is not configured.");
    }

    let mut prompt = if let Some(ref q) = query {
        format!(
            "Analyze these Rust vending machines matching '{}'. \
            Are the deals worth it? Highlight the best items. \
            Be minimal and straightforward. Do not use emojis.\n\n\
            Inventory:\n\n",
            q
        )
    } else {
        String::from(
            "Analyze these Rust vending machine deals based on standard economics (sulfur/scrap > wood/stone). \
            Find the 5 absolute BEST deals for value. \
            Be minimal and straightforward. Do not use emojis.\n\n\
            Inventory:\n\n",
        )
    };

    let mut item_count = 0;
    for marker in vending_machines {
        if marker.sell_orders.is_empty() {
            continue;
        }

        let grid = get_grid_pos(marker.x, marker.y, map_size);
        let name = marker.name.as_deref().unwrap_or("Vending Machine");

        // Filter by query if provided (checking grid or name)
        if let Some(ref q) = query {
            let q_lower = q.to_lowercase();
            if !grid.to_lowercase().contains(&q_lower) && !name.to_lowercase().contains(&q_lower) {
                continue;
            }
        }

        let mut has_stock = false;
        let mut machine_text = format!("**{name}** at {grid}:\n");

        for order in &marker.sell_orders {
            if order.amount_in_stock > 0 {
                has_stock = true;
                let item_name = get_item_name(order.item_id);
                let currency_name = get_item_name(order.currency_id);
                machine_text.push_str(&format!(
                    " - Sells {}x {} for {}x {} (Stock: {})\n",
                    order.quantity,
                    item_name,
                    order.cost_per_item,
                    currency_name,
                    order.amount_in_stock
                ));
                item_count += 1;
            }
        }

        if has_stock {
            prompt.push_str(&machine_text);
            prompt.push('\n');
        }
    }

    if item_count == 0 {
        if let Some(ref q) = query {
            return Ok(format!("No stocked vending machines found matching '{q}'."));
        } else {
            return Ok("No stocked vending machines found to analyze.".to_string());
        }
    }

    let request_body = GeminiRequest {
        system_instruction: Some(GeminiContent {
            role: Some("system".to_string()),
            parts: vec![GeminiPart::Text {
                text: "You are an AI assistant for a Rust server. Be minimal and straightforward. Do not use emojis.".to_string(),
            }],
        }),
        contents: vec![GeminiContent {
            role: Some("user".to_string()),
            parts: vec![GeminiPart::Text { text: prompt }],
        }],
        tools: None,
    };

    let client = Client::new();
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={}",
        api_key
    );

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .context("Failed to send request to Gemini API")?;

    let gemini_resp: GeminiResponse = response
        .json()
        .await
        .context("Failed to parse Gemini API JSON response")?;

    if let Some(err) = gemini_resp.error {
        anyhow::bail!("Gemini API Error: {}", err.message);
    }

    let text = gemini_resp
        .candidates
        .and_then(|mut c| c.pop())
        .and_then(|c| c.content)
        .and_then(|mut content| content.parts.pop())
        .and_then(|p| match p {
            GeminiPart::Text { text } => Some(text),
            _ => None,
        })
        .context("Could not extract text from Gemini response")?;

    Ok(text)
}

pub async fn chat_with_tools(
    server_id: i32,
    user_message: &str,
    map_size: u32,
    vending_machines: &[AppMarker],
) -> Result<String> {
    let api_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
    if api_key.is_empty() || api_key == "your_gemini_api_key_here" {
        anyhow::bail!("GEMINI_API_KEY is not configured.");
    }

    let mut properties = std::collections::HashMap::new();
    properties.insert(
        "query".to_string(),
        ParameterProperty {
            type_: "STRING".to_string(),
            description: "Optional search query (grid code or item name)".to_string(),
        },
    );

    let tools = vec![GeminiTool {
        function_declarations: vec![FunctionDeclaration {
            name: "get_vending_deals".to_string(),
            description: "Get the best vending machine deals currently available on the server, optionally filtering by grid or item.".to_string(),
            parameters: Some(FunctionParameters {
                type_: "OBJECT".to_string(),
                properties,
            }),
        }],
    }];

    let mut request_body = GeminiRequest {
        system_instruction: Some(GeminiContent {
            role: Some("system".to_string()),
            parts: vec![GeminiPart::Text {
                text: "You are an AI assistant for a Rust server. Be minimal and straightforward. Do not use emojis.".to_string(),
            }],
        }),
        contents: vec![GeminiContent {
            role: Some("user".to_string()),
            parts: vec![GeminiPart::Text { text: user_message.to_string() }],
        }],
        tools: Some(tools.clone()),
    };

    let client = Client::new();
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key={}",
        api_key
    );

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .context("Failed to send first request to Gemini API")?;

    let gemini_resp: GeminiResponse = response
        .json()
        .await
        .context("Failed to parse first Gemini API JSON response")?;

    if let Some(err) = gemini_resp.error {
        anyhow::bail!("Gemini API Error: {}", err.message);
    }

    let candidate_content = gemini_resp
        .candidates
        .and_then(|mut c| c.pop())
        .and_then(|c| c.content)
        .context("Could not extract content from Gemini response")?;

    // Check if the response contains a function call
    if let Some(part) = candidate_content.parts.first() {
        if let GeminiPart::FunctionCall { function_call } = part {
            if function_call.name == "get_vending_deals" {
                // Execute the tool
                let query = function_call
                    .args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .map(String::from);

                let tool_result =
                    match find_best_deals(server_id, map_size, vending_machines, query).await {
                        Ok(deals) => deals,
                        Err(e) => format!("Error fetching deals: {}", e),
                    };

                // Add the model's function call response to the history
                request_body.contents.push(candidate_content.clone());

                // Add the function response
                request_body.contents.push(GeminiContent {
                    role: Some("function".to_string()),
                    parts: vec![GeminiPart::FunctionResponse {
                        function_response: FunctionResponse {
                            name: "get_vending_deals".to_string(),
                            response: FunctionResponseData {
                                content: tool_result,
                            },
                        },
                    }],
                });

                // Make the second API call
                let second_response = client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .json(&request_body)
                    .send()
                    .await
                    .context("Failed to send second request to Gemini API")?;

                let second_gemini_resp: GeminiResponse = second_response
                    .json()
                    .await
                    .context("Failed to parse second Gemini API JSON response")?;

                if let Some(err) = second_gemini_resp.error {
                    anyhow::bail!("Gemini API Error: {}", err.message);
                }

                let final_text = second_gemini_resp
                    .candidates
                    .and_then(|mut c| c.pop())
                    .and_then(|c| c.content)
                    .and_then(|mut content| content.parts.pop())
                    .and_then(|p| match p {
                        GeminiPart::Text { text } => Some(text),
                        _ => None,
                    })
                    .context("Could not extract final text from Gemini response")?;

                return Ok(final_text);
            }
        } else if let GeminiPart::Text { text } = part {
            return Ok(text.clone());
        }
    }

    anyhow::bail!("Unexpected response format from Gemini");
}
