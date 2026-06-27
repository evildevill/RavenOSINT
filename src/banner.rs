use colored::*;

pub fn print_banner(no_color: bool) {
    if no_color {
        print!("{}", PLAIN_BANNER);
    } else {
        print_colored_banner();
    }
}

const PLAIN_BANNER: &str = r"
╔══════════════════════════════════════════════════════╗
║                                                     ║
║    ██████╗   █████╗  ██╗   ██╗ ███████╗ ███╗   ██╗ ║
║    ██╔══██╗ ██╔══██╗ ██║   ██║ ██╔════╝ ████╗  ██║ ║
║    ██████╔╝ ███████║ ██║   ██║ ███████╗ ██╔██╗ ██║ ║
║    ██╔══██╗ ██╔══██║ ╚██╗ ██╔╝ ╚════██║ ██║╚██╗██║ ║
║    ██║  ██║ ██║  ██║  ╚████╔╝  ███████║ ██║ ╚████║ ║
║    ╚═╝  ╚═╝ ╚═╝  ╚═╝   ╚═══╝   ╚══════╝ ╚═╝  ╚═══╝ ║
║                                                     ║
║          OSINT Username Search Engine                ║
║                                                     ║
╚══════════════════════════════════════════════════════╝
";

fn print_colored_banner() {
    for line in PLAIN_BANNER.lines() {
        if line.starts_with('\u{2554}') || line.starts_with('\u{255a}') {
            println!("  {}", line.bright_blue());
        } else if line.contains("OSINT Username Search Engine") {
            let colored = line.replace("OSINT Username Search Engine", &"OSINT Username Search Engine".white().bold().to_string());
            println!("  {}", colored.bright_blue());
        } else {
            println!("  {}", line.bright_blue());
        }
    }
}
