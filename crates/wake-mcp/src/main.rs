mod protocol;
mod server;
mod tools;

use protocol::{JsonRpcRequest, JsonRpcResponse, PARSE_ERROR};
use server::McpServer;
use std::io::{BufRead, Write};
use wake_core::Database;

fn main() {
    // Open database
    let db = match Database::open() {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Failed to open database: {e}");
            std::process::exit(1);
        }
    };

    let mut server = McpServer::new(db);
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    // Read JSON-RPC messages line by line from stdin
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Error reading stdin: {e}");
                break;
            }
        };

        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        // Parse JSON-RPC request
        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let response =
                    JsonRpcResponse::error(None, PARSE_ERROR, format!("Parse error: {e}"));
                if let Ok(json) = serde_json::to_string(&response) {
                    let _ = writeln!(stdout, "{json}");
                    let _ = stdout.flush();
                }
                continue;
            }
        };

        // Handle request
        if let Some(response) = server.handle_message(request) {
            if let Ok(json) = serde_json::to_string(&response) {
                let _ = writeln!(stdout, "{json}");
                let _ = stdout.flush();
            }
        }
    }
}
