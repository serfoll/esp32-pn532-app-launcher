// A small in-memory event log for the output log drawer -- reader state,
// tag events, launches, binds, and errors, timestamped. Not persisted;
// resets each session, same as any debug console.

const MAX_LOG_LINES = 200;
const MAX_CONSECUTIVE_REPEATS = 3;

let lines: string[] = [];
let lastMessage: string | null = null;
let repeatCount = 0;

/** A tag sitting in a marginal read spot can fire the same event (e.g.
 * "Tag removed: X") dozens of times a second. Past MAX_CONSECUTIVE_REPEATS
 * identical messages in a row, further repeats are dropped silently; the
 * moment a different message comes in, the counter resets and logging
 * resumes normally. */
export function appendLog(container: HTMLElement, message: string): void {
  repeatCount = message === lastMessage ? repeatCount + 1 : 1;
  lastMessage = message;

  if (repeatCount > MAX_CONSECUTIVE_REPEATS) {
    return;
  }

  const timestamp = new Date().toLocaleTimeString();
  lines.push(`[${timestamp}] ${message}`);
  if (lines.length > MAX_LOG_LINES) {
    lines = lines.slice(-MAX_LOG_LINES);
  }
  container.textContent = lines.join("\n");
  container.scrollTop = container.scrollHeight;
}
