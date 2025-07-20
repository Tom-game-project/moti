use rust_editor::run;
use std::io;

#[tokio::main]
async fn main() -> io::Result<()> {
    run().await
}
