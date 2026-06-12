use plotters::prelude::*;
use std::io::Cursor;
use image::{ImageBuffer, RgbImage, ImageFormat};
use anyhow::Result;

pub fn generate_activity_chart(daily_data: &[(String, f64)]) -> Result<Vec<u8>> {
    let width = 600;
    let height = 300;
    
    let mut buffer = vec![0u8; (width * height * 3) as usize];
    
    {
        let root = BitMapBackend::with_buffer(&mut buffer, (width, height)).into_drawing_area();
        // Discord dark gray: #2B2D31
        root.fill(&RGBColor(43, 45, 49))?;
        
        if daily_data.is_empty() {
            let style = TextStyle::from(("sans-serif", 20).into_font()).color(&WHITE);
            root.draw_text("No activity data available", &style, (170, 140))?;
            root.present()?;
        } else {
            let max_val = daily_data.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max);
            let max_y = (max_val * 1.2).max(1.0);
            
            let mut chart = ChartBuilder::on(&root)
                .margin(20)
                .x_label_area_size(30)
                .y_label_area_size(40)
                .build_cartesian_2d(0..daily_data.len(), 0.0..max_y)?;
                
            chart.configure_mesh()
                .disable_x_mesh()
                .y_desc("Hours Played")
                .y_label_style(("sans-serif", 15).into_font().color(&WHITE))
                .x_label_style(("sans-serif", 12).into_font().color(&WHITE))
                .axis_style(&WHITE)
                .x_label_formatter(&|x| {
                    if *x < daily_data.len() {
                        daily_data[*x].0.split('-').last().unwrap_or("").to_string()
                    } else {
                        String::new()
                    }
                })
                .draw()?;
                
            chart.draw_series(
                Histogram::vertical(&chart)
                    .style(RGBColor(88, 101, 242).filled()) // Blurple color
                    .data(daily_data.iter().enumerate().map(|(i, (_, v))| (i, *v))),
            )?;
            
            root.present()?;
        }
    }
    
    let img: RgbImage = ImageBuffer::from_raw(width, height, buffer).unwrap();
    let mut png_bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)?;
    
    Ok(png_bytes)
}
