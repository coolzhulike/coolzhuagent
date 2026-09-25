use std::env;
use std::time::Duration;

use computer_use::input::{
    click_point, mouse_button_action_point, preflight_report, MouseButtonAction,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("preflight") | None => {
            let report = preflight_report();
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Some("click") => {
            let x = parse_arg::<i32>(&mut args, "x")?;
            let y = parse_arg::<i32>(&mut args, "y")?;
            let clicks = args
                .next()
                .map(|value| value.parse::<u32>())
                .transpose()?
                .unwrap_or(1);
            click_point(x, y, clicks, Duration::from_secs(6))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "action": "click",
                    "x": x,
                    "y": y,
                    "clicks": clicks,
                    "backend": preflight_report().backend,
                }))?
            );
        }
        Some("mouse-action") => {
            let action = parse_mouse_action(&parse_arg::<String>(&mut args, "action")?)?;
            let x = parse_arg::<i32>(&mut args, "x")?;
            let y = parse_arg::<i32>(&mut args, "y")?;
            mouse_button_action_point(x, y, action, Duration::from_secs(6))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "action": format!("{action:?}"),
                    "x": x,
                    "y": y,
                    "backend": preflight_report().backend,
                }))?
            );
        }
        Some(command) => {
            return Err(format!(
                "unknown command: {command}; expected `preflight`, `click x y [clicks]`, or `mouse-action left|right|chord|double x y`"
            )
            .into());
        }
    }
    Ok(())
}

fn parse_mouse_action(value: &str) -> Result<MouseButtonAction, Box<dyn std::error::Error>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "left" | "left-click" => Ok(MouseButtonAction::LeftClick),
        "right" | "right-click" => Ok(MouseButtonAction::RightClick),
        "chord" | "left-right" | "left-right-chord" => Ok(MouseButtonAction::LeftRightChord),
        "double" | "double-click" => Ok(MouseButtonAction::DoubleClick),
        other => Err(format!("unsupported mouse action: {other}").into()),
    }
}

fn parse_arg<T>(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<T, Box<dyn std::error::Error>>
where
    T: std::str::FromStr,
    T::Err: std::error::Error + 'static,
{
    let value = args.next().ok_or_else(|| format!("missing {name}"))?;
    Ok(value.parse::<T>()?)
}
