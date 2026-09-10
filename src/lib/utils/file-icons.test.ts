import { describe, it, expect } from 'vitest';

import { getFileIcon } from './file-icons';

/**
 * The icon a file gets, as a question about **which rule won**.
 *
 * An icon table is mostly data and data does not need a test. What does is the *order* of the
 * rules in `getFileIcon`: an exact name beats a pattern, a pattern beats the extension, and every
 * one of those precedences exists because some file was getting the wrong mark. Comparing icons
 * by identity rather than asserting a shape keeps this about the routing, which is the part that
 * can break.
 */
const iconFor = (name: string, rootTag?: string) => getFileIcon(name, rootTag);
const same = (a: string, b: string) => expect(iconFor(a)).toBe(iconFor(b));
const different = (a: string, b: string) => expect(iconFor(a)).not.toBe(iconFor(b));

describe('getFileIcon', () => {
  it('gives JSP and its family one mark, apart from any other markup', () => {
    same('index.jsp', 'fragment.jspf');
    same('index.jsp', 'header.tag');
    same('index.jsp', 'header.tagx');
    different('index.jsp', 'beans.xml');
    different('index.jsp', 'index.html');
  });

  it('picks Struts config out of the forty other XMLs around it', () => {
    same('struts.xml', 'struts-security.xml');
    same('struts.xml', 'struts.properties');
    different('struts.xml', 'beans.xml');
    // `validation.xml` is Struts's AND Bean Validation's — claiming a framework would be a guess.
    same('validation.xml', 'beans.xml');
  });

  it('marks Tomcat by name and leaves the servlet spec alone', () => {
    same('context.xml', 'server.xml');
    same('context.xml', 'tomcat-users.xml');
    // `web.xml` is the servlet descriptor, identical under Jetty or WebSphere.
    same('web.xml', 'beans.xml');
    different('context.xml', 'web.xml');
  });

  /**
   * The tier that exists because a name is not evidence: Tomcat's per-application context is
   * `context.xml` in a `.war` and `<appname>.xml` under `conf/Catalina/localhost`, and a Struts
   * module configuration is called whatever the constant says.
   */
  it('reads what an XML says it is, whatever it is called', () => {
    expect(iconFor('portale-appalti.xml', 'Context')).toBe(iconFor('context.xml'));
    expect(iconFor('conf.xml', 'struts')).toBe(iconFor('struts.xml'));
    expect(iconFor('legacy.xml', 'struts-config')).toBe(iconFor('struts.xml'));
    // And an XML that says nothing recognisable stays an XML.
    expect(iconFor('data.xml', 'catalog')).toBe(iconFor('beans.xml'));
  });

  it('lets a recognised root element overrule a name that guessed wrong', () => {
    // Named like a Struts config and it is a Tomcat context: the content wins.
    expect(iconFor('struts-context.xml', 'Context')).toBe(iconFor('context.xml'));
    expect(iconFor('struts-context.xml', 'Context')).not.toBe(iconFor('struts.xml'));
  });

  /**
   * And the limit of that, which is deliberate: only roots that identify a file *unambiguously*
   * are in the table. `<beans>` is Spring's and also CDI's, so it earns no mark — and a name that
   * did guess is then still the best answer available.
   */
  it('does not let an unrecognised root undo what the name knew', () => {
    expect(iconFor('struts-orders.xml', 'beans')).toBe(iconFor('struts.xml'));
  });

  it('still answers from the name where no content is to hand', () => {
    // A tab strip has a path and no bytes — the name tiers are what is left.
    expect(iconFor('struts-orders.xml')).toBe(iconFor('struts.xml'));
    expect(iconFor('context.xml')).toBe(iconFor('server.xml'));
  });

  /** An exact name is a deliberate convention and outranks even the content. */
  it('keeps an exact name above the root element', () => {
    expect(iconFor('pom.xml', 'project')).toBe(iconFor('pom.xml'));
  });

  it('gives batch its own mark, shared between the two names of one file', () => {
    same('build.bat', 'deploy.cmd');
    different('build.bat', 'build.sh');
  });

  it('lets an exact name beat the extension it happens to have', () => {
    // The precedence that matters most in a Java tree: a pom is not "one XML among forty".
    different('pom.xml', 'beans.xml');
    different('lombok.config', 'app.config');
    different('junit-platform.properties', 'messages.properties');
    // And a Dockerfile with a suffix is still a Dockerfile, not a `.dev`.
    same('Dockerfile', 'Dockerfile.jvm');
  });

  it('falls back to one generic mark for anything it has no rule for', () => {
    same('notes.unknownextension', 'anything.else');
  });
});
