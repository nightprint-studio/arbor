//! The assistant's own tool definitions: the few things Bennu's catalogue does not cover.

use super::*;

impl ToolHost {
    /// Every tool the assistant defines itself (search, find, edit, create, delete, refactorings, plan, help, ask).
    pub(super) fn own_specs() -> Vec<ToolSpec> {
        let spec = |name: &str, description: &str, schema: Value| ToolSpec {
            name: name.into(),
            description: description.into(),
            input_schema: schema,
        };
        vec![
            spec(
                "search_text",
                "Search the text of files for a string or a regular expression, like grep. Returns matching lines with their file and line number, optionally with context. Skips build output, version-control folders and binaries. Prefer Bennu's symbol tools (usages, symbols) when the question is about code structure; use this for strings, comments, configuration and anything the index does not model.",
                json!({"type":"object","properties":{
                    "pattern":{"type":"string","description":"Text or regular expression to look for."},
                    "path":{"type":"string","description":"Folder or file to search; absolute or relative to the project. Defaults to the project root."},
                    "regex":{"type":"boolean","description":"Treat pattern as a regular expression. Default false (plain text)."},
                    "ignore_case":{"type":"boolean"},
                    "extensions":{"type":"array","items":{"type":"string"},"description":"Only files with these extensions, without the dot."},
                    "context":{"type":"integer","description":"Lines of context around each match, 0-5."},
                    "max_results":{"type":"integer","description":"Stop after this many matching lines. Default 100."},
                    "files_only":{"type":"boolean","description":"Only list the files that match, with counts."},
                    "project":{"type":"string","description":"With several projects open: a project's name, or \"all\", to search there instead of in the one the user is in."}
                },"required":["pattern"]}),
            ),
            spec(
                "find_files",
                "Find files by name, like find. `name` is part of the file name, or `*.ext` for a suffix. Skips build output and version-control folders.",
                json!({"type":"object","properties":{
                    "name":{"type":"string"},
                    "path":{"type":"string","description":"Folder to search; defaults to the project root."},
                    "max_results":{"type":"integer"},
                    "project":{"type":"string","description":"With several projects open: a project's name, or \"all\"."}
                },"required":["name"]}),
            ),
            spec(
                "edit_file",
                "Replace exact text in a file. `old_text` must appear in the file exactly once (include enough surrounding lines to make it unique) unless `expected_count` says how many to replace. The file is read and written in its own encoding. Depending on the user's mode the edit is applied at once or proposed for the user to review; the result says which. Read the file first.",
                json!({"type":"object","properties":{
                    "file":{"type":"string","description":"Absolute path, or relative to the project."},
                    "old_text":{"type":"string"},
                    "new_text":{"type":"string"},
                    "expected_count":{"type":"integer","description":"How many occurrences to replace. Default 1."}
                },"required":["file","old_text","new_text"]}),
            ),
            spec(
                "create_file",
                "Create a new file with the given text. Fails if the file already exists — use edit_file to change one.",
                json!({"type":"object","properties":{
                    "file":{"type":"string","description":"Absolute path, or relative to the project."},
                    "text":{"type":"string"}
                },"required":["file","text"]}),
            ),
            spec(
                "rename_symbol",
                "Rename a symbol — a class, method, field, variable or parameter — and everything that refers to it, across the whole project, using Bennu's reference index (not text replacement). Point at the symbol with the file and the line and column of its name (a declaration or any use). A public type's file is renamed too. Proposed or applied like any edit, as one change. Prefer this to editing names by hand.",
                json!({"type":"object","properties":{
                    "file":{"type":"string","description":"Absolute path, or relative to the project, of a file containing the symbol."},
                    "line":{"type":"integer","description":"1-based line of the symbol's name."},
                    "column":{"type":"integer","description":"1-based column within the line. Default 1; give it when the line has several names."},
                    "new_name":{"type":"string"}
                },"required":["file","line","new_name"]}),
            ),
            spec(
                "add_parameter",
                "Add a parameter to a Java method and update the method, its overriders and every call, using Bennu's reference index. Point at the method's name (its declaration or a call) with file, line and column. Callers receive `default_value` as the argument. Refused, with the reason, when the change is not safe (a method reference, a library override). Proposed or applied as one change.",
                json!({"type":"object","properties":{
                    "file":{"type":"string"},
                    "line":{"type":"integer","description":"1-based line of the method's name."},
                    "column":{"type":"integer","description":"1-based column of the method's name. Default 1."},
                    "type":{"type":"string","description":"The parameter's type as written, e.g. `Currency` or `List<String>`."},
                    "name":{"type":"string"},
                    "default_value":{"type":"string","description":"What existing calls pass. Defaults to the type's zero value (0, false, null)."},
                    "before":{"type":"integer","description":"Index of the parameter it goes before; absent means last."}
                },"required":["file","line","type","name"]}),
            ),
            spec(
                "delete_file",
                "Delete files or folders. The user is always asked first, and what is deleted is kept in Local History so it can be restored.",
                json!({"type":"object","properties":{
                    "paths":{"type":"array","items":{"type":"string"},"description":"Absolute paths, or relative to the project."}
                },"required":["paths"]}),
            ),
            spec(
                "update_plan",
                "Keep a visible plan for work that has three or more steps. Call it at the start with every step as `pending` and the first as `in_progress`, then call it again as you go: mark a step `completed` as soon as it is done and the next `in_progress`. Exactly one step is in progress at a time. Send the whole list each time. Add steps if you find more to do. Call it in the same turn as the other tools you are using, never on its own. Do not use it for a question or a one-step change — the user reads this as progress, so keep each step short and true.",
                json!({"type":"object","properties":{"steps":{"type":"array","minItems":1,"maxItems":12,"items":{"type":"object","properties":{
                    "text":{"type":"string","description":"The step, in a few words."},
                    "status":{"type":"string","enum":["pending","in_progress","completed"]}
                },"required":["text","status"]}}},"required":["steps"]}),
            ),
            spec(
                "bennu_help",
                "Search Bennu's own documentation. Use it whenever the user asks how to do something in Bennu itself — a feature, a shortcut, a setting, where a panel is, how a refactoring or a run configuration works. Pass a few words for what they want to do. It returns the matching sections with the page each is from; answer from them, and if nothing matches say you could not find it rather than guessing how Bennu works.",
                json!({"type":"object","properties":{
                    "query":{"type":"string","description":"A few words: what the user wants to do, e.g. \"rename a symbol\", \"run configuration\", \"split the editor\"."},
                    "limit":{"type":"integer","description":"How many sections, 1-6. Default 4."}
                },"required":["query"]}),
            ),
            spec(
                "ask_user",
                "Ask the user one to four multiple-choice questions when a decision is theirs to make and guessing would be wrong. Each question has 2-4 options; the user can always type their own answer instead. Returns their answers.",
                json!({"type":"object","properties":{"questions":{"type":"array","minItems":1,"maxItems":4,"items":{"type":"object","properties":{
                    "header":{"type":"string","description":"One or two words naming the decision."},
                    "text":{"type":"string","description":"The question."},
                    "options":{"type":"array","items":{"type":"object","properties":{"label":{"type":"string"},"description":{"type":"string"}},"required":["label"]}},
                    "multi":{"type":"boolean","description":"Allow choosing several."}
                },"required":["header","text","options"]}}},"required":["questions"]}),
            ),
        ]
    }
}
