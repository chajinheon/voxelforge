//! Deterministic WGSL source composition. WGSL has no project-local include syntax.

pub const PBR_COMMON_WGSL: &str = include_str!("../../assets/shaders/pbr_common.wgsl");

pub fn compose(pass: &str) -> String {
    let mut source = String::with_capacity(PBR_COMMON_WGSL.len() + pass.len() + 1);
    source.push_str(PBR_COMMON_WGSL);
    source.push('\n');
    source.push_str(pass);
    source
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_precedes_pass() {
        let s = compose("@compute fn main() {}");
        assert!(s.find("const PI").unwrap() < s.find("@compute").unwrap());
    }
}
