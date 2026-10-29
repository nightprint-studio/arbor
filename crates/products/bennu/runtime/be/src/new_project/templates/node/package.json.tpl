{
  "name": "{{name}}",
  "version": "1.0.0",
  "description": "",
  "type": "{{module_type}}",
  "main": "{{entry}}",
  "scripts": {
{{#if ts}}
    "build": "tsc",
    "start": "node dist/index.js",
    "dev": "tsc --watch"
{{/if}}
{{#unless ts}}
    "start": "node index.js"
{{/unless}}
  },
{{#if ts}}
  "devDependencies": {
    "@types/node": "^22.0.0",
    "typescript": "^5.6.0"
  },
{{/if}}
  "license": "{{license}}"
}
