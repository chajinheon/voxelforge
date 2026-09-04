pub(crate) fn save_name() -> anyhow::Result<String> {
    let mut name = std::env::var("VF_SAVE").unwrap_or_else(|_| "default".to_string());
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--save" {
            name = args
                .next()
                .ok_or_else(|| anyhow::anyhow!("--save requires a name"))?;
        } else if let Some(value) = arg.strip_prefix("--save=") {
            name = value.to_string();
        }
    }
    Ok(name)
}
