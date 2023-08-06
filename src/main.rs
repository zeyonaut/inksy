#![feature(array_windows)]

mod app;
mod render;
mod stroke;
mod wintab;

use app::App;
use winit::event_loop::EventLoopBuilder;

// Program entry point.
fn main() {
	// Set up the event logger.
	env_logger::init();

	// Initialize the event loop.
	let event_loop = EventLoopBuilder::new().build();

	// Initialize the app at the event loop.
	let app = App::new(&event_loop);

	// Run the app with its event loop.
	app.run(event_loop);
}
