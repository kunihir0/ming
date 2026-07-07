use axum::{Router, extract::Query, response::Html, routing::get};
use push_receiver::android_fcm::AndroidFcm;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::process::Command;
use tokio::sync::oneshot;
use uuid::Uuid;

const HTML_CONTENT: &str = r#"
<html lang="en">
<head>
    <title>RustPlus Pairing</title>
</head>
<body>
<div>To pair with Rust+, you must allow popup windows. You may need to refresh this page after enabling popups.</div>
<script type="text/javascript">
    var popupWindow = window.open("https://companion-rust.facepunch.com/login", "", "");
    var handlerInterval = setInterval(function() {
        if(popupWindow.ReactNativeWebView === undefined){
            console.log("registering ReactNativeWebView.postMessage handler");
            popupWindow.ReactNativeWebView = {
                postMessage: function(message) {
                    clearInterval(handlerInterval);
                    var auth = JSON.parse(message);
                    window.location.href = "http://localhost:3000/callback?token=" + encodeURIComponent(auth.Token);
                    popupWindow.close();
                },
            };
        }
    }, 250);
</script>
</body>
</html>
"#;

#[derive(Deserialize)]
struct CallbackQuery {
    token: String,
}

#[derive(Serialize)]
struct ExpoPushRequest {
    r#type: &'static str,
    #[serde(rename = "deviceId")]
    device_id: String,
    development: bool,
    #[serde(rename = "appId")]
    app_id: &'static str,
    #[serde(rename = "deviceToken")]
    device_token: String,
    #[serde(rename = "projectId")]
    project_id: &'static str,
}

#[derive(Deserialize)]
struct ExpoData {
    expoPushToken: String,
}

#[derive(Deserialize)]
struct ExpoPushResponse {
    data: ExpoData,
}

#[derive(Serialize)]
struct FpRegisterRequest {
    #[serde(rename = "AuthToken")]
    auth_token: String,
    #[serde(rename = "DeviceId")]
    device_id: &'static str,
    #[serde(rename = "PushKind")]
    push_kind: i32,
    #[serde(rename = "PushToken")]
    push_token: String,
}

#[derive(Serialize)]
struct OutputJson {
    fcm_credentials: push_receiver::android_fcm::AndroidFcmRegistration,
    expo_push_token: String,
    rustplus_auth_token: String,
}

async fn get_expo_push_token(client: &Client, fcm_token: String) -> anyhow::Result<String> {
    let req = ExpoPushRequest {
        r#type: "fcm",
        device_id: Uuid::new_v4().to_string(),
        development: false,
        app_id: "com.facepunch.rust.companion",
        device_token: fcm_token,
        project_id: "49451aca-a822-41e6-ad59-955718d0ff9c",
    };

    let res: ExpoPushResponse = client
        .post("https://exp.host/--/api/v2/push/getExpoPushToken")
        .json(&req)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(res.data.expoPushToken)
}

async fn register_with_rustplus(
    client: &Client,
    auth_token: String,
    expo_push_token: String,
) -> anyhow::Result<()> {
    let req = FpRegisterRequest {
        auth_token,
        device_id: "rustplus.rs",
        push_kind: 3,
        push_token: expo_push_token,
    };

    client
        .post("https://companion-rust.facepunch.com:443/api/push/register")
        .json(&req)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}

fn launch_chrome() -> anyhow::Result<()> {
    let url = "http://localhost:3000";
    let user_data_dir = std::env::temp_dir().join("temporary-chrome-profile-dir-rustplus");

    // Try Windows first
    if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(&[
                "/C",
                "start",
                "msedge",
                url,
                "--disable-web-security",
                "--disable-popup-blocking",
                "--disable-site-isolation-trials",
                &format!("--user-data-dir={}", user_data_dir.display()),
            ])
            .spawn()?;
    } else if cfg!(target_os = "macos") {
        Command::new("open")
            .args(&[
                "-a",
                "Google Chrome",
                url,
                "--args",
                "--disable-web-security",
                "--disable-popup-blocking",
                "--disable-site-isolation-trials",
                &format!("--user-data-dir={}", user_data_dir.display()),
            ])
            .spawn()?;
    } else {
        Command::new("google-chrome")
            .args(&[
                url,
                "--disable-web-security",
                "--disable-popup-blocking",
                "--disable-site-isolation-trials",
                &format!("--user-data-dir={}", user_data_dir.display()),
            ])
            .spawn()?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    println!("Registering with FCM...");
    let client = Client::new();

    let fcm_credentials = AndroidFcm::register(
        &client,
        "AIzaSyB5y2y-Tzqb4-I4Qnlsh_9naYv_TD8pCvY",
        "rust-companion-app",
        "976529667804",
        "1:976529667804:android:d6f1ddeb4403b338fea619",
        "com.facepunch.rust.companion",
        "E28D05345FB78A7A1A63D70F4A302DBF426CA5AD",
    )
    .await?;

    println!("Fetching Expo Push Token...");
    let expo_push_token = get_expo_push_token(&client, fcm_credentials.fcm.token.clone()).await?;

    println!("Google Chrome is launching so you can link your Steam account with Rust+...");

    let (tx, rx) = oneshot::channel::<String>();
    let tx = std::sync::Arc::new(tokio::sync::Mutex::new(Some(tx)));

    let app = Router::new()
        .route("/", get(|| async { Html(HTML_CONTENT) }))
        .route(
            "/callback",
            get({
                let tx = tx.clone();
                move |Query(query): Query<CallbackQuery>| async move {
                    if let Some(sender) = tx.lock().await.take() {
                        let _ = sender.send(query.token);
                    }
                    "Steam Account successfully linked with Rust+. You can now close this window and go back to the console."
                }
            }),
        );

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    launch_chrome()?;

    let server = axum::serve(listener, app);

    // Wait for the token
    let rustplus_auth_token = tokio::select! {
        token = rx => token?,
        _ = server => anyhow::bail!("Server closed unexpectedly"),
    };

    println!("Registering with Rust Companion API...");
    register_with_rustplus(
        &client,
        rustplus_auth_token.clone(),
        expo_push_token.clone(),
    )
    .await?;

    let output = OutputJson {
        fcm_credentials,
        expo_push_token,
        rustplus_auth_token,
    };

    println!("\n================== CREDENTIALS ==================\n");
    println!("{}", serde_json::to_string_pretty(&output)?);
    println!("\n=================================================\n");
    println!("Please copy the JSON block above and paste it into your Rust+ Dashboard.");

    Ok(())
}
