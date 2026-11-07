/** Gallery scene overrides and extra scenes for the "editor" feature area — see gallery/registry.ts. */
import { mount } from 'svelte';
import type { SceneOverride, ExtraScene } from './scenes';

export const editorOverrides: Record<string, SceneOverride> = {};

const JAVA = `package it.comune.gestionale.core.util;

import java.util.regex.Pattern;

public class InjectedSamples {
    // language=SQL
    static final String REPORT = """
        SELECT b.id, b.titolo, COUNT(a.id) AS allegati
          FROM bando b
          LEFT JOIN allegato a ON a.bando_id = b.id
         WHERE b.stato = 'APERTO' AND b.scadenza > CURRENT_DATE
         GROUP BY b.id, b.titolo
         ORDER BY allegati DESC
        """;

    // language=RegExp
    static final String ISO_DATE = "(?<year>\\d{4})-(?<month>0[1-9]|1[0-2])-(?<day>[0-3]\\d)";

    static final Pattern CODICE = Pattern.compile("^[A-Z]{2}-\\d{3,6}(?:/[a-z]+)?$");

    // language=HTML
    static final String CARD = "<div class=\\"card\\" id=\\"bando-1\\"><h2>Titolo</h2><a href=\\"/bandi/1\\">Apri</a></div>";

    // language=JavaScript
    static final String SCRIPT = "document.querySelectorAll('.card').forEach(function (el) { el.addEventListener('click', () => alert(el.id)); });";

    // language=CSS
    static final String STYLE = ".card { display: flex; padding: 8px 12px; color: #336699; border: 1px solid rgba(0,0,0,.2); }";

    // language=Lua
    static final String LUA = "local total = 0 for i = 1, 10 do total = total + i end return total";
}
`;

const JSP = `<%@ page contentType="text/html; charset=UTF-8" %>
<%@ taglib prefix="c" uri="http://java.sun.com/jsp/jstl/core" %>
<%@ taglib prefix="wp" uri="/aps-core" %>
<div id="note">
  <h2><wp:i18n key="LABEL_NOTE" /></h2>
</div>
<script>
  // espande/comprime le note di categoria
  <c:if test="\${noteVisibili}">
  $('[id^="funznota"]').on("click", function(){
    var cat = $(this).attr('id').substring(8);
    if ($(this).hasClass('expand')) {
      $('textarea[id="nota'+cat+'"]').fadeIn('slow');
      <c:if test="\${skin == 'highcontrast' || skin == 'text'}">$(this).text('(<wp:i18n key="LABEL_NOTA_CATEGORIA_COMPRIMI" />)');</c:if>
      $(this).attr('title', '<wp:i18n key="LABEL_NOTA_CATEGORIA_COMPRIMI" />');
    }
  });
  </c:if>
</script>
`;

/** The shared code editor with the real language descriptor for \`path\`, in a fixed-size box. */
const editorScene = (path: string, text: string, w = 1100, h = 760): ExtraScene['load'] => async () => {
  const CodeEditor = (await import('$lib/components/shared/ui/code-editor/CodeEditor.svelte')).default;
  const { languageForPath } = await import('$lib/components/bennu/languages');
  const Frame = (anchor: Node) => {
    const box = document.createElement('div');
    box.style.cssText = `width:${w}px;height:${h}px;margin:24px;display:flex;flex-direction:column;overflow:hidden;background:var(--bg-base);border:1px solid var(--border-subtle, #333);border-radius:var(--radius-lg, 8px)`;
    anchor.parentNode?.insertBefore(box, anchor);
    mount(CodeEditor, { target: box, props: { value: text, language: languageForPath(path), readOnly: true } });
  };
  return { default: Frame as any };
};

/** A Patina mod script (the Voriz mod language): every token kind the stream mode draws. */
const PATINA = `use voriz::prelude::*
use crate::tally::Tally

const POWER: int = 3

/// Una mina: chi la calpesta senza accovacciarsi salta in aria.
#[server]
#[on(StepOn, block: "landmine")]
fn arm(ev: StepOn, tally: &mut Tally) -> Result<(), WriteRefused> {
    if ev.who.is_sneaking() {
        return Ok(())
    }
    tally.count(ev.pos)
    let mut near = world.players_near(ev.pos.center(), 8.0)
        .filter(|p| p.name() != ev.who.name())
        .count()
    'outer: for p in world.players() {
        match tally.mood {
            Mood::Calm => break 'outer,
            Mood::Scared { since } => near += 1,
        }
    }
    world.set_block(ev.pos, block!("air"))?
    ev.who.send(t!("marmo:boom", power: POWER * 2))
    Ok(())
}

impl Tally {
    pub fn count(&mut self, at: BlockPos) {
        self.sprung += 1
        self.last = Some(BlockPos::new(at.x, at.y + 1, at.z))
    }
}
`;

export const editorExtras: ExtraScene[] = [
  { id: 'bennu/editor/InjectedLanguages', load: editorScene('/demo/InjectedSamples.java', JAVA), settle: 1200 },
  { id: 'bennu/editor/JspScript', load: editorScene('/demo/notes.jsp', JSP, 1200, 520), settle: 1200 },
  { id: 'bennu/editor/Patina', load: editorScene('/mods/marmo/script/mine.ptn', PATINA, 1100, 760), settle: 800 },
];
