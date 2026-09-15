# bennu-i18n

Message bundles as a first-class model, contributed to Bennu through the [`bennu-ext`](../ext)
framework-extension seam.

```rust
use bennu_i18n::prelude::*;

let catalog = BundleCatalog::build(&files);          // every .properties, grouped by bundle
catalog.declarations("login.title");                  // one per locale, default first
catalog.untranslated("login.title");                  // the locales that still owe it
keys_in("/p/login.jsp", source);                      // every key the page reads
```

A host registers `MessagesExtension` and asks nothing else of this crate — go-to, hover,
completion, the unknown-key diagnostic and the `keys` catalog all arrive through the trait.

## Why it exists

Half of what a legacy web app puts on screen is not in its source: it is in a `.properties` file,
reached by a string, checked by nothing. Struts renders an unresolved key as the key itself, so a
typo does not throw — it ships, and a user reads `note.login.expiredPassword.intro` off a login
page.

## What counts as a key reference

By **shape**, not by a list of tags, because six frameworks spell this six ways and a legacy app
uses at least three of them:

| Shape | Covers |
|---|---|
| an attribute called `key` | `<fmt:message>`, `<bean:message>`, `<wp:i18n>`, `<html:errors>`, `<message key>` in a validator |
| an attribute ending in `Key` | `<display:column titleKey>`, `messageKey`, `labelKey` |
| `name` on a `*:text` tag | Struts 2's `<s:text name>` — the one tag where `name` is a key |
| the first string argument of `getText` / `getMessage` / `getString` | actions, services, anything Java |

A **computed** value (`%{keyName}`, `${row.label}`, a scriptlet) is not a reference. It usually is
one at runtime, but nothing here can say which, and guessing would flag every dynamic label in the
project.

## The locale rule

A file's locale is only read off its name when the suffix has the shape of one — a two-or-three
letter lowercase language, optionally followed by a two-letter uppercase country. So
`labels_admin.properties` is a bundle called `labels_admin`, not the `admin` translation of
`labels`. The failure mode that leaves is one bundle too many, never a key filed under the wrong
name.

## Layout

| Module | Holds |
|---|---|
| `bundle` | one `.properties` file: locale split, the Java key/value grammar, byte spans |
| `catalog` | every bundle indexed by key; declarations, locales, untranslated |
| `refs` | where a key is read, and the half-typed prefix a completion continues |
| `ext` | the `FrameworkExtension` impl |
