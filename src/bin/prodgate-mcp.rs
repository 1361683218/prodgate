//! MCP stdio server for prodgate. One JSON-RPC message per line, responses on
//! stdout, diagnostics on stderr so they never corrupt the protocol stream.

use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("prodgate-mcp: stdin read error: {}", e);
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = prodgate::mcp::handle(&line) {
            if let Err(e) = writeln!(out, "{}", response) {
                eprintln!("prodgate-mcp: stdout write error: {}", e);
                break;
            }
            let _ = out.flush();
        }
    }
}
