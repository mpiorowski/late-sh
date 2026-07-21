fn main() -> std::io::Result<()> {
    let mut edge_to_edge = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--edge2edge" => edge_to_edge = true,
            "-h" | "--help" => {
                println!(
                    "Usage: leaderboard_tui [--edge2edge]\n\n  --edge2edge  remove the simulated outer terminal margins"
                );
                return Ok(());
            }
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("unknown argument: {argument}"),
                ));
            }
        }
    }

    late_ssh::leaderboard_preview::run(edge_to_edge)
}
