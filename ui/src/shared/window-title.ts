/**
 * What the session on screen is doing, as the desktop window's title says it. The bridge's
 * `window.title` turns it and the session's name into the title's text, from the same Rust code
 * that writes the terminal's title (`bravebot_agent::title`).
 */
export type TitleState = 'ready' | 'working' | 'waiting'
