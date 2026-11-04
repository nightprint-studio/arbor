/** Gallery scene overrides and extra scenes for the "gen" feature area — see gallery/registry.ts. */
import type { SceneOverride, ExtraScene } from './scenes';
import type { GenSite } from '$lib/ipc/bennu/generate';
import type { GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';

const noop = () => {};
const DEMO = 'demo://GestionaleAtti';
const BANDO = `${DEMO}/core/src/main/java/it/comune/gestionale/core/model/Bando.java`;
const SERVICE = `${DEMO}/core/src/main/java/it/comune/gestionale/core/service/BandoServiceImpl.java`;
const CONTROLLER = `${DEMO}/web/src/main/java/it/comune/gestionale/web/controller/BandoController.java`;

const JAVA_SOURCE = 'package it.comune.gestionale.core.model;\n\n@Entity\npublic class Bando {\n    @Id\n    private Long id;\n    private String titolo;\n}\n';
const RUST_SOURCE = 'use tauri::State;\n\npub struct AppState;\n\n';

/** A host whose caret sits in `file` of the demo Spring Boot project (or a Rust project). */
const hostAt = (site: GenSite): GeneratorHost => ({ site: () => site, applyEdits: noop, openFile: noop });
const javaAt = (file: string): GenSite => ({ root: DEMO, file, source: JAVA_SOURCE, offset: 60 });
const rustAt = (): GenSite => ({ root: '/Users/dev/projects/tauri-app', file: '/Users/dev/projects/tauri-app/src-tauri/src/commands/project.rs', source: RUST_SOURCE, offset: RUST_SOURCE.length });

const scene = (site: GenSite): SceneOverride => ({ props: () => ({ host: hostAt(site), onClose: noop }), settle: 1500 });

export const genOverrides: Record<string, SceneOverride> = {
  'bennu/generate/SecurityGenerateModal': scene(javaAt(CONTROLLER)),
  'bennu/generate/ConverterGenerateModal': scene(javaAt(BANDO)),
  'bennu/generate/JacksonGenerateModal': scene(javaAt(BANDO)),
  'bennu/generate/HttpClientGenerateModal': scene(javaAt(CONTROLLER)),
  'bennu/generate/DtoMapperGenerateModal': scene(javaAt(BANDO)),
  'bennu/generate/BuilderGenerateModal': scene(javaAt(BANDO)),
  'bennu/generate/ValueMembersGenerateModal': scene(javaAt(BANDO)),
  'bennu/generate/TestSkeletonGenerateModal': scene(javaAt(SERVICE)),
  'bennu/generate/TauriCommandGenerateModal': scene(rustAt()),
  'bennu/generate/BevyGenerateModal': scene(rustAt()),
};

export const genExtras: ExtraScene[] = [];
