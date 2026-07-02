# bennu-web

The Bennu **web-config graph** — **Phase-0 skeleton**.

Role (docs §2, §8): model the web-config control flow as a first-class language (the
XML *is* the source of truth — docs §8):

- **Struts2 / XWork** — `struts.xml` + the per-classpath `<include>` graph, wildcard
  expansion (→ *candidate* nav, marked inferred — docs §7), `validation.xml`.
- **Apache Tiles** — result → def → JSP resolution.
- **Spring bean-XML** — `id` → impl (docs §10 C1: `<action class="beanId">` is a
  bean-id, not an FQCN, so JSP→action resolution goes through here).
- **Entando-aware** — merges the config fragments pulled by classpath-name.

Skeleton only: role + prelude. The graph builders land in the Phase-2 config-index
wave, feeding `bennu-index`.
