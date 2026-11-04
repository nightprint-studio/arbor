/**
 * The `.http` / `.rest` language for the editor: a request is a method and a URL, then headers, a blank line and
 * a body; `###` separates requests; `{{variables}}` are coloured wherever they are; `<` and `>` lines (a body
 * from a file, a response handler or file) are marked. A legacy stream parser, like the other small formats.
 */
import type { StreamParser } from '@codemirror/language';

type Line = 'plain' | 'url' | 'header';

interface State {
  /** Where in a request we are: before it, in its headers, in its body. */
  phase: 'start' | 'headers' | 'body';
  line: Line;
}

/** What a new `.http` file starts with. */
export const NEW_HTTP_FILE = `# Run a request with Ctrl+Enter, or the ▶ beside it. Requests are separated by ###.
# Environments: http-client.env.json beside this file. Variables: @name = value, or {{$uuid}}.
@host = https://httpbin.org

### Get something
GET {{host}}/get
Accept: application/json

###
POST {{host}}/post
Content-Type: application/json

{ "id": "{{$uuid}}" }
`;

const METHOD = /^(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS|TRACE|CONNECT)(?=\s|$)/;

export const httpParser: StreamParser<State> = {
  name: 'http',
  startState: () => ({ phase: 'start', line: 'plain' }),

  token(stream, state) {
    if (stream.sol()) {
      state.line = 'plain';
      if (stream.match(/^\s*###.*/)) {
        state.phase = 'start';
        return 'heading';
      }
      if (stream.match(/^\s*(#|\/\/).*/)) return 'comment';
      if (stream.match(/^\s*@[^\s=]+(?=\s*=)/)) return 'def';
      if (stream.eol() || stream.match(/^\s*$/)) {
        if (state.phase === 'headers') state.phase = 'body';
        return null;
      }
      if (state.phase === 'body') {
        if (stream.match(/^\s*(<>|<|>>!?|>)(?=\s)/)) {
          stream.skipToEnd();
          return 'meta';
        }
      } else if (state.phase === 'start') {
        state.phase = 'headers';
        state.line = 'url';
        if (stream.match(METHOD)) return 'keyword';
      } else if (stream.match(/^[A-Za-z0-9!#$%&'*+.^_`|~-]+(?=\s*:)/)) {
        state.line = 'header';
        return 'propertyName';
      }
    }

    if (stream.match(/\{\{[^}]*\}\}/)) return 'variableName';
    if (stream.eatSpace()) return null;
    if (state.line === 'url') {
      if (stream.match(/^HTTP\/\d(\.\d)?/)) return 'atom';
      stream.eatWhile(/[^\s{]/);
      return stream.current() ? 'string' : (stream.next(), null);
    }
    if (state.line === 'header' && stream.match(':')) return 'punctuation';
    stream.eatWhile(/[^{\s]/);
    if (!stream.current()) stream.next();
    return null;
  },

  languageData: { commentTokens: { line: '#' } },
};
