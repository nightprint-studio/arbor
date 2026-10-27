// What each detected capability *means*, for the Frameworks tab.
//
// The backend reports capabilities as a flat set of booleans (`CapabilitySet`) plus the evidence
// behind each. A name like `spring_data_repo` is not an answer to "why is this feature not here" —
// what helps is saying in words what the capability is and what makes Bennu decide it is present,
// so that a missing one can be fixed (add the dependency, add the file) rather than puzzled over.
//
// Only what is true of the detection is written here, not a list of features per capability: those
// change faster than this file would. A key the backend adds that is not here still shows up — under
// "Other", labelled from its name — so this catalogue can lag without hiding anything.

export type CapabilityGroup = 'web' | 'spring' | 'persistence' | 'language' | 'testing' | 'platform' | 'engine' | 'other';

export const GROUP_ORDER: CapabilityGroup[] = ['web', 'spring', 'persistence', 'language', 'testing', 'platform', 'engine', 'other'];

export const GROUP_LABEL: Record<CapabilityGroup, string> = {
  web: 'Web',
  spring: 'Spring & DI',
  persistence: 'Persistence',
  language: 'Language & data binding',
  testing: 'Testing',
  platform: 'Platform',
  engine: 'Engines & conventions',
  other: 'Other',
};

export interface CapabilityInfo {
  label: string;
  group: CapabilityGroup;
  /** What it is, in a phrase. */
  what: string;
  /** What Bennu looks for — the thing to add when it is missing. */
  looksFor: string;
}

export const CAPABILITIES: Record<string, CapabilityInfo> = {
  struts_xml_config: { label: 'Struts (struts.xml)', group: 'web', what: 'Struts 2 actions declared in XML.', looksFor: 'a struts.xml file, or the Struts 2 dependency.' },
  struts_convention: { label: 'Struts convention plugin', group: 'web', what: 'Struts 2 actions mapped by naming convention.', looksFor: 'the Struts convention plugin on the classpath.' },
  jsp_views: { label: 'JSP views', group: 'web', what: 'The project has JSP pages at all.', looksFor: '*.jsp, *.jspf or *.tag files. Weaker than a tag-library descriptor; gates the JSP-only tooling (Forms, Tomcat deploy).' },
  jsp_taglib_tld: { label: 'JSP tag libraries', group: 'web', what: 'Tag libraries described by .tld files.', looksFor: '.tld descriptors in the project or its dependencies.' },
  ognl_value_stack: { label: 'OGNL value stack', group: 'web', what: 'OGNL expressions resolved against the action.', looksFor: 'OGNL in JSPs or the Struts/OGNL dependencies.' },
  tiles_views: { label: 'Tiles', group: 'web', what: 'Apache Tiles page composition.', looksFor: 'a Tiles definitions file or the Tiles dependency.' },

  spring_xml_di: { label: 'Spring XML beans', group: 'spring', what: 'Beans declared in Spring XML.', looksFor: 'Spring bean definition files.' },
  spring_annotation_di: { label: 'Spring annotations', group: 'spring', what: 'Beans declared with stereotype annotations.', looksFor: '@Component, @Service, @Configuration… or Spring on the classpath.' },
  spring_data_repo: { label: 'Spring Data repositories', group: 'spring', what: 'Repository interfaces with derived queries.', looksFor: 'Spring Data on the classpath, or repository interfaces.' },
  scheduling: { label: 'Scheduling', group: 'spring', what: 'Scheduled work.', looksFor: 'Quartz, @Scheduled or @EnableScheduling.' },

  jpa_hibernate: { label: 'JPA / Hibernate', group: 'persistence', what: 'Entities mapped to tables.', looksFor: 'a JPA or Hibernate dependency, or entity annotations in the sources.' },
  mybatis_mapper: { label: 'MyBatis', group: 'persistence', what: 'SQL mapped through MyBatis mappers.', looksFor: 'MyBatis mapper interfaces or mapper XML.' },
  jdbc_dao: { label: 'JDBC / DAO', group: 'persistence', what: 'Hand-written data access over JDBC.', looksFor: 'JDBC usage in DAO-style classes.' },

  lombok: { label: 'Lombok', group: 'language', what: 'Generated members (getters, builders…) the sources never spell out.', looksFor: 'the Lombok dependency or its annotations.' },
  bean_validation: { label: 'Bean Validation', group: 'language', what: 'Constraint annotations on your types.', looksFor: 'the API, an engine, Spring’s validation starter, a validation bundle, or constraints in the sources.' },
  jackson: { label: 'Jackson', group: 'language', what: 'JSON binding.', looksFor: 'jackson-databind or a starter that brings it, or Jackson annotations in the sources.' },
  mapstruct: { label: 'MapStruct', group: 'language', what: 'Generated bean mappers.', looksFor: 'org.mapstruct on the classpath or imported in the sources.' },

  mockito: { label: 'Mockito', group: 'testing', what: 'Mocks in tests.', looksFor: 'a mockito-* artifact, Spring Boot’s test starter, or org.mockito in the sources.' },
  assertj: { label: 'AssertJ', group: 'testing', what: 'Fluent assertions.', looksFor: 'assertj-core, Spring Boot’s test starter, or org.assertj in the sources.' },

  jakarta_ee: { label: 'Jakarta EE / Java EE', group: 'platform', what: 'The enterprise platform APIs.', looksFor: 'the platform API, CDI, EJB or a CDI container; jakarta.enterprise, javax.ejb or @WebServlet in the sources.' },
  jaxrs: { label: 'JAX-RS', group: 'platform', what: 'REST endpoints through JAX-RS.', looksFor: 'jakarta.ws.rs / javax.ws.rs, Jersey, RESTEasy or CXF.' },

  entando_japs: { label: 'Entando / JAPS', group: 'engine', what: 'The Entando (JAPS) portal conventions.', looksFor: 'Entando / JAPS artifacts and descriptors.' },
  fulcrum_i18n: { label: 'Fulcrum i18n', group: 'engine', what: 'The fulcrum engine’s translation bundles.', looksFor: 'an i18n/languages.toml with <lang>/<category>.toml bundles beside it. Read from the layout, so a project that only authors content has it.' },
  bevy: { label: 'Bevy ECS', group: 'engine', what: 'Bevy entities, components and systems.', looksFor: 'a bevy / bevy_* dependency in a Cargo manifest, corroborated by the sources.' },
};

/** A label for a key the catalogue does not know yet: `some_new_cap` → `Some New Cap`. */
export function fallbackLabel(field: string): string {
  return field.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

export function infoOf(field: string): CapabilityInfo {
  return CAPABILITIES[field] ?? {
    label: fallbackLabel(field),
    group: 'other',
    what: '',
    looksFor: '',
  };
}
