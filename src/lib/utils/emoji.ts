// GitHub/GitLab emoji shortcode → Unicode mapping.
//
// Both providers render shortcodes like `:warning:` and `:white_check_mark:`
// inline; bots (GitLab Security Bot, dependabot, etc.) lean on them heavily,
// so the renderer must turn them into real emojis instead of letting them
// surface as literal text.
//
// Curated subset — the real GitHub list has 1500+ entries and most are
// noise. Add new ones as they show up in real PRs/MRs.

const EMOJI_MAP: Record<string, string> = {
  // Status / severity — most common in bot output
  warning: '⚠️',
  exclamation: '❗',
  heavy_exclamation_mark: '❗',
  bangbang: '‼️',
  question: '❓',
  grey_question: '❔',
  no_entry: '⛔',
  no_entry_sign: '🚫',
  white_check_mark: '✅',
  heavy_check_mark: '✔️',
  ballot_box_with_check: '☑️',
  x: '❌',
  negative_squared_cross_mark: '❎',
  o: '⭕',
  information_source: 'ℹ️',
  bell: '🔔',
  no_bell: '🔕',
  lock: '🔒',
  unlock: '🔓',
  shield: '🛡️',

  // Reactions / sentiment
  '+1': '👍',
  '-1': '👎',
  thumbsup: '👍',
  thumbsdown: '👎',
  ok_hand: '👌',
  clap: '👏',
  pray: '🙏',
  wave: '👋',
  eyes: '👀',
  thinking: '🤔',
  tada: '🎉',
  rocket: '🚀',
  fire: '🔥',
  '100': '💯',
  sparkles: '✨',
  star: '⭐',
  star2: '🌟',
  heart: '❤️',
  broken_heart: '💔',
  smile: '😄',
  grinning: '😀',
  laughing: '😆',
  joy: '😂',
  cry: '😢',
  sob: '😭',
  rage: '😡',
  poop: '💩',
  shrug: '🤷',

  // Dev / commit-message conventions
  bug: '🐛',
  ant: '🐜',
  zap: '⚡',
  hammer: '🔨',
  wrench: '🔧',
  hammer_and_wrench: '🛠️',
  gear: '⚙️',
  art: '🎨',
  package: '📦',
  pencil: '✏️',
  pencil2: '✏️',
  memo: '📝',
  books: '📚',
  book: '📖',
  scroll: '📜',
  clipboard: '📋',
  page_facing_up: '📄',
  link: '🔗',
  paperclip: '📎',
  mag: '🔍',
  mag_right: '🔎',
  bookmark: '🔖',
  label: '🏷️',
  bulb: '💡',
  boom: '💥',
  recycle: '♻️',
  construction: '🚧',
  truck: '🚚',
  rotating_light: '🚨',
  test_tube: '🧪',
  microscope: '🔬',

  // Time / progress
  hourglass: '⌛',
  hourglass_flowing_sand: '⏳',
  watch: '⌚',
  clock1: '🕐',
  alarm_clock: '⏰',
  stopwatch: '⏱️',

  // Arrows
  arrow_up: '⬆️',
  arrow_down: '⬇️',
  arrow_left: '⬅️',
  arrow_right: '➡️',
  leftwards_arrow_with_hook: '↩️',
  arrow_right_hook: '↪️',

  // Misc
  robot: '🤖',
  bot: '🤖',
  ghost: '👻',
  alien: '👽',
  computer: '💻',
  floppy_disk: '💾',
  open_file_folder: '📂',
  file_folder: '📁',
  globe_with_meridians: '🌐',
  earth_americas: '🌎',
  earth_africa: '🌍',
  earth_asia: '🌏',
  speech_balloon: '💬',
  thought_balloon: '💭',
};

/** Replace `:shortcode:` tokens with their Unicode emoji.
 *  Unknown shortcodes are left intact (so a stray `:not_an_emoji:` keeps
 *  rendering as text rather than getting silently eaten). The regex bounds
 *  the shortcode to word characters + `+`/`-`/`_` to avoid eating things
 *  like `https://example.com:80/foo` or `: spaced :`. */
export function replaceEmojiShortcodes(text: string): string {
  return text.replace(/:([a-z0-9_+-]+):/gi, (m, code: string) => {
    const lower = code.toLowerCase();
    return EMOJI_MAP[lower] ?? m;
  });
}
