// No console window, in debug builds too: the app is also launched by Windows as a browser
// handler and spawns helper processes, and a terminal popping up for those is unacceptable.
#![windows_subsystem = "windows"]

fn main() {
    unimail_lib::run()
}
