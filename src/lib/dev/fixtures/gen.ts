import type { FixtureHandler } from './index';
import type { ClassInfo, EndpointInfo, GeneratorMeta, GeneratorPreview, OpenedGenerator } from '$lib/ipc/bennu/generate';

/** Mock backend answers for the "gen" gallery scenes, keyed by `program.method`. */

const DEMO = 'demo://GestionaleAtti';
const MAIN = `${DEMO}/core/src/main/java/it/comune/gestionale/core`;
const PKG = 'it.comune.gestionale.core.model';

const meta = (id: string, title: string, language: 'java' | 'rust', icon: string): GeneratorMeta => ({
  id, title, description: title, icon, group: 'Generate', language, targets: ['class'], related: [],
});

const BANDO: ClassInfo = {
  name: 'Bando',
  package: PKG,
  kind: 'class',
  annotations: ['@Entity', '@Table(name = "BANDO")'],
  fields: [
    { name: 'id', type_name: 'Long', is_final: false, id: true, annotations: ['@Id', '@GeneratedValue'] },
    { name: 'titolo', type_name: 'String', is_final: false, id: false, annotations: ['@Column(nullable = false)'] },
    { name: 'stato', type_name: 'StatoBando', is_final: false, id: false, annotations: ['@Enumerated(EnumType.STRING)'] },
    { name: 'scadenza', type_name: 'LocalDate', is_final: false, id: false, annotations: [] },
    { name: 'importo', type_name: 'BigDecimal', is_final: false, id: false, annotations: [] },
    { name: 'allegati', type_name: 'List<Allegato>', is_final: false, id: false, annotations: ['@OneToMany'] },
  ],
  constants: [],
  methods: ['getId', 'getTitolo', 'setTitolo', 'getStato', 'chiudi', 'isScaduto'],
};

const SERVICE: ClassInfo = {
  ...BANDO,
  name: 'BandoServiceImpl',
  package: 'it.comune.gestionale.core.service',
  annotations: ['@Service'],
  fields: [],
  methods: ['trovaPerId', 'elenca', 'crea', 'chiudiBando', 'elimina'],
};

const endpoint = (methods: string[], path: string, handler: string, ret: string, params: EndpointInfo['params'] = []): EndpointInfo => ({
  id: `BandoController#${handler}`, methods, path, controller: 'BandoController', handler, return_type: ret, produces: 'application/json', params,
});

const ENDPOINTS: EndpointInfo[] = [
  endpoint(['GET'], '/api/bandi', 'elenca', 'List<BandoDto>'),
  endpoint(['GET'], '/api/bandi/{id}', 'dettaglio', 'BandoDto', [{ name: 'id', type_text: 'Long', binding: 'path', required: true }]),
  endpoint(['POST'], '/api/bandi', 'crea', 'BandoDto', [{ name: 'body', type_text: 'BandoDto', binding: 'body', required: true }]),
  endpoint(['PUT'], '/api/bandi/{id}', 'aggiorna', 'BandoDto'),
  endpoint(['POST'], '/api/bandi/{id}/chiusura', 'chiudi', 'void'),
  endpoint(['DELETE'], '/api/bandi/{id}', 'elimina', 'void'),
].map((e, i) => ({ ...e, id: `BandoController#${e.handler}${i}` }));

const SPRING = ['Spring Boot 3.2.1', 'Spring Security 6.2', 'jakarta', 'Java 21', 'Lombok', 'Maven'];

const opened = (
  m: GeneratorMeta, defaults: Record<string, unknown>, detected: string[], cls: ClassInfo | null, endpoints: EndpointInfo[] = [],
): OpenedGenerator => ({ meta: m, defaults, detected, class: cls, package: cls?.package ?? PKG, endpoints });

const OPENED: Record<string, OpenedGenerator> = {
  'spring-security': opened(meta('spring-security', 'Spring Security configuration', 'java', 'ShieldCheck'), {
    class_name: 'SecurityConfig', package: 'it.comune.gestionale.web.config', scenario: 'jwt', style: 'auto',
    swagger: true, health: true, cors: true, cors_origins: ['https://app.comune.example'], cors_credentials: true,
    csrf: false, csrf_cookie: false, method_security: true, user_store: 'memory', password_encoder: 'bcrypt',
    h2_console: false, rest_entry_point: true, jwt_source: 'issuer',
    issuers: [{ issuer_uri: 'https://sso.comune.example/realms/gestionale', audience: 'gestionale-api', roles_claim: 'realm_access.roles' }],
    rules: [
      { method: 'GET', path: '/api/bandi/**', access: 'hasAuthority("SCOPE_read")' },
      { method: 'DELETE', path: '/api/bandi/**', access: 'hasRole("ADMIN")' },
    ],
    any_request: 'authenticated()', authority_prefix: 'ROLE_',
  }, [...SPRING, 'oauth2-resource-server'], null),
  'attribute-converter': opened(meta('attribute-converter', 'JPA attribute converter', 'java', 'ArrowLeftRight'), {
    kind: 'date-string', java_type: 'LocalDate', db_type: 'String', class_name: 'LocalDateStringConverter', package: `${PKG}.converter`,
    auto_apply: true, null_safe: true, field: 'scadenza', separator: ',', true_value: 'Y', false_value: 'N',
    enum_mode: 'name', code_getter: '', key_env: 'CONVERTER_KEY', to_expr: '', from_expr: '',
  }, ['JPA 3.1', 'Hibernate 6.4', 'jakarta', 'Java 21'], BANDO),
  'jackson-serde': opened(meta('jackson-serde', 'Jackson serializer / deserializer', 'java', 'Braces'), {
    type_name: 'Bando', type_package: PKG, mode: 'both', representation: 'object', registration: 'json-component', naming: 'snake_case',
    include_nulls: false, to_string_expr: 'value.toString()', from_string_expr: 'new Bando(text)', class_name: 'BandoJson',
    package: `${PKG}.json`, field: '',
  }, ['Spring Boot 3.2.1', 'Jackson 2.16', 'jakarta', 'Java 21'], BANDO),
  'http-client': opened(meta('http-client', 'HTTP client from Spring endpoints', 'java', 'Network'), {
    library: 'restclient', endpoints: ENDPOINTS.slice(0, 4).map((e) => e.id), class_name: 'BandoClient',
    package: 'it.comune.gestionale.client', base_url_property: 'gestionale.bandi', base_url_default: 'http://localhost:8080',
    auth: 'bearer', api_key_header: 'X-Api-Key', dtos: 'reuse', errors: 'exception', timeout_seconds: 30, reactive: false, records: true,
  }, ['Spring Boot 3.2.1', 'Spring Web 6.1', 'RestClient', 'Java 21', `${ENDPOINTS.length} endpoints`], null, ENDPOINTS),
  'dto-mapper': opened(meta('dto-mapper', 'DTO and mapper', 'java', 'Shuffle'), {
    dto_name: 'BandoDto', mapper_name: 'BandoMapper', package: `${PKG}.dto`, entity_package: PKG, entity_name: 'Bando',
    fields: ['id', 'titolo', 'stato', 'scadenza', 'importo'], dto_kind: 'auto', mapper: 'auto', to_entity: true, list_methods: true,
  }, ['MapStruct 1.5', 'Lombok', 'Java 21', 'JPA entity'], BANDO),
  builder: opened(meta('builder', 'Builder', 'java', 'Hammer'), {
    style: 'nested', fields: ['id', 'titolo', 'stato', 'scadenza', 'importo'], to_builder: true, required: ['titolo'], factory: 'builder',
  }, ['Java 21', 'Lombok'], BANDO),
  'value-members': opened(meta('value-members', 'equals, hashCode and toString', 'java', 'Hash'), {
    equals: true, hash_code: true, to_string: true, fields: ['id'], class_check: 'auto', use_getters: false,
  }, ['JPA entity', 'Java 21'], BANDO),
  'test-skeleton': opened(meta('test-skeleton', 'Test class', 'java', 'FlaskConical'), {
    kind: 'auto', junit: '5', assertions: 'assertj', methods: ['trovaPerId', 'chiudiBando'], class_name: '',
  }, ['JUnit 5', 'AssertJ', 'Mockito 5', 'Spring Boot Test'], SERVICE),
  'tauri-command': opened(meta('tauri-command', 'Tauri command', 'rust', 'Plug'), {
    name: 'load_project', params: [{ name: 'path', ty: 'String' }, { name: 'recursive', ty: 'bool' }], returns: 'ProjectInfo',
    fallible: true, async_fn: true, state: 'AppState', app_handle: true, window: false, public: true,
  }, ['Tauri 2.1', 'serde', 'tokio', 'Rust 2021'], null),
  'bevy-skeleton': opened(meta('bevy-skeleton', 'Bevy component, system or plugin', 'rust', 'Boxes'), {
    kind: 'system', name: 'move_players', fields: [{ name: 'speed', ty: 'f32' }], query: ['&mut Transform', '&Player'],
    filters: ['Without<Frozen>'], resources: ['Res<Time>'], commands: false, update_systems: [], startup_systems: [],
    init_resources: [], events: [], bevy_minor: 16,
  }, ['Bevy 0.16', 'Rust 2021'], null),
};

const JAVA_SECURITY = `package it.comune.gestionale.web.config;

import java.util.List;

import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.core.convert.converter.Converter;
import org.springframework.http.HttpMethod;
import org.springframework.security.authentication.AbstractAuthenticationToken;
import org.springframework.security.config.Customizer;
import org.springframework.security.config.annotation.method.configuration.EnableMethodSecurity;
import org.springframework.security.config.annotation.web.builders.HttpSecurity;
import org.springframework.security.config.annotation.web.configurers.AbstractHttpConfigurer;
import org.springframework.security.config.http.SessionCreationPolicy;
import org.springframework.security.oauth2.jwt.Jwt;
import org.springframework.security.web.SecurityFilterChain;
import org.springframework.web.cors.CorsConfiguration;
import org.springframework.web.cors.CorsConfigurationSource;
import org.springframework.web.cors.UrlBasedCorsConfigurationSource;

@Configuration
@EnableMethodSecurity
public class SecurityConfig {

    @Bean
    public SecurityFilterChain securityFilterChain(HttpSecurity http) throws Exception {
        http
            .cors(Customizer.withDefaults())
            .csrf(AbstractHttpConfigurer::disable)
            .sessionManagement(s -> s.sessionCreationPolicy(SessionCreationPolicy.STATELESS))
            .authorizeHttpRequests(auth -> auth
                .requestMatchers("/swagger-ui/**", "/v3/api-docs/**").permitAll()
                .requestMatchers("/actuator/health").permitAll()
                .requestMatchers(HttpMethod.GET, "/api/bandi/**").hasAuthority("SCOPE_read")
                .requestMatchers(HttpMethod.DELETE, "/api/bandi/**").hasRole("ADMIN")
                .anyRequest().authenticated())
            .oauth2ResourceServer(oauth -> oauth
                .jwt(jwt -> jwt.jwtAuthenticationConverter(jwtAuthenticationConverter())));
        return http.build();
    }

    @Bean
    public CorsConfigurationSource corsConfigurationSource() {
        CorsConfiguration config = new CorsConfiguration();
        config.setAllowedOrigins(List.of("https://app.comune.example"));
        config.setAllowedMethods(List.of("GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"));
        config.setAllowedHeaders(List.of("*"));
        config.setAllowCredentials(true);
        UrlBasedCorsConfigurationSource source = new UrlBasedCorsConfigurationSource();
        source.registerCorsConfiguration("/**", config);
        return source;
    }
}
`;

const JAVA_CONVERTER = `package it.comune.gestionale.core.model.converter;

import java.time.LocalDate;

import jakarta.persistence.AttributeConverter;
import jakarta.persistence.Converter;

/** Stores a {@link LocalDate} as ISO-8601 text (yyyy-MM-dd). */
@Converter(autoApply = true)
public class LocalDateStringConverter implements AttributeConverter<LocalDate, String> {

    @Override
    public String convertToDatabaseColumn(LocalDate attribute) {
        return attribute == null ? null : attribute.toString();
    }

    @Override
    public LocalDate convertToEntityAttribute(String dbData) {
        return dbData == null ? null : LocalDate.parse(dbData);
    }
}
`;

const JAVA_JACKSON = `package it.comune.gestionale.core.model.json;

import java.io.IOException;

import com.fasterxml.jackson.core.JsonGenerator;
import com.fasterxml.jackson.core.JsonParser;
import com.fasterxml.jackson.databind.DeserializationContext;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.SerializerProvider;
import org.springframework.boot.jackson.JsonComponent;

import it.comune.gestionale.core.model.Bando;

@JsonComponent
public class BandoJson {

    public static class Serializer extends com.fasterxml.jackson.databind.JsonSerializer<Bando> {
        @Override
        public void serialize(Bando value, JsonGenerator gen, SerializerProvider provider) throws IOException {
            gen.writeStartObject();
            gen.writeNumberField("id", value.getId());
            gen.writeStringField("titolo", value.getTitolo());
            gen.writeStringField("stato", value.getStato().name());
            gen.writeStringField("scadenza", value.getScadenza().toString());
            gen.writeNumberField("importo", value.getImporto());
            gen.writeEndObject();
        }
    }

    public static class Deserializer extends com.fasterxml.jackson.databind.JsonDeserializer<Bando> {
        @Override
        public Bando deserialize(JsonParser p, DeserializationContext ctxt) throws IOException {
            JsonNode node = p.getCodec().readTree(p);
            Bando result = new Bando();
            result.setId(node.path("id").asLong());
            result.setTitolo(node.path("titolo").asText(null));
            return result;
        }
    }
}
`;

const JAVA_CLIENT = `package it.comune.gestionale.client;

import java.util.List;

import org.springframework.beans.factory.annotation.Value;
import org.springframework.core.ParameterizedTypeReference;
import org.springframework.stereotype.Component;
import org.springframework.web.client.RestClient;

import it.comune.gestionale.core.dto.BandoDto;

/** Typed client for BandoController. The base URL comes from gestionale.bandi.base-url. */
@Component
public class BandoClient {

    private final RestClient client;

    public BandoClient(RestClient.Builder builder,
                       @Value("\${gestionale.bandi.base-url:http://localhost:8080}") String baseUrl) {
        this.client = builder.baseUrl(baseUrl).build();
    }

    public List<BandoDto> elenca(String bearerToken) {
        return client.get()
            .uri("/api/bandi")
            .headers(h -> h.setBearerAuth(bearerToken))
            .retrieve()
            .onStatus(s -> s.isError(), (req, res) -> { throw new BandoClientException(res.getStatusCode().value(), res.getBody()); })
            .body(new ParameterizedTypeReference<List<BandoDto>>() {});
    }

    public BandoDto dettaglio(String bearerToken, Long id) {
        return client.get()
            .uri("/api/bandi/{id}", id)
            .headers(h -> h.setBearerAuth(bearerToken))
            .retrieve()
            .body(BandoDto.class);
    }

    public BandoDto crea(String bearerToken, BandoDto body) {
        return client.post()
            .uri("/api/bandi")
            .headers(h -> h.setBearerAuth(bearerToken))
            .body(body)
            .retrieve()
            .body(BandoDto.class);
    }
}
`;

const JAVA_DTO = `package it.comune.gestionale.core.model.dto;

import java.math.BigDecimal;
import java.time.LocalDate;

import it.comune.gestionale.core.model.StatoBando;

public record BandoDto(Long id, String titolo, StatoBando stato, LocalDate scadenza, BigDecimal importo) {}
`;

const JAVA_MAPPER = `package it.comune.gestionale.core.model.dto;

import java.util.List;

import org.mapstruct.Mapper;

import it.comune.gestionale.core.model.Bando;

@Mapper(componentModel = "spring")
public interface BandoMapper {

    BandoDto toDto(Bando entity);

    Bando toEntity(BandoDto dto);

    List<BandoDto> toDtos(List<Bando> entities);
}
`;

const JAVA_BUILDER = `public static Builder builder() {
    return new Builder();
}

public Builder toBuilder() {
    return new Builder().id(id).titolo(titolo).stato(stato).scadenza(scadenza).importo(importo);
}

public static final class Builder {
    private Long id;
    private String titolo;
    private StatoBando stato;
    private LocalDate scadenza;
    private BigDecimal importo;

    private Builder() {}

    public Builder id(Long id) { this.id = id; return this; }
    public Builder titolo(String titolo) { this.titolo = titolo; return this; }
    public Builder stato(StatoBando stato) { this.stato = stato; return this; }
    public Builder scadenza(LocalDate scadenza) { this.scadenza = scadenza; return this; }
    public Builder importo(BigDecimal importo) { this.importo = importo; return this; }

    public Bando build() {
        Bando result = new Bando();
        result.id = Objects.requireNonNull(id, "id");
        result.titolo = Objects.requireNonNull(titolo, "titolo");
        result.stato = stato;
        result.scadenza = scadenza;
        result.importo = importo;
        return result;
    }
}
`;

const JAVA_VALUE = `@Override
public boolean equals(Object o) {
    if (this == o) return true;
    if (o == null || getClass() != o.getClass()) return false;
    Bando other = (Bando) o;
    return Objects.equals(id, other.id);
}

@Override
public int hashCode() {
    return Objects.hash(id);
}

@Override
public String toString() {
    return "Bando{id=" + id + "}";
}
`;

const JAVA_TEST = `package it.comune.gestionale.core.service;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.Mockito.when;

import java.util.Optional;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.InjectMocks;
import org.mockito.Mock;
import org.mockito.junit.jupiter.MockitoExtension;

@ExtendWith(MockitoExtension.class)
class BandoServiceImplTest {

    @Mock
    private BandoDao bandoDao;

    @InjectMocks
    private BandoServiceImpl service;

    @Test
    void trovaPerId() {
        when(bandoDao.findById(1L)).thenReturn(Optional.empty());
        assertThat(service.trovaPerId(1L)).isNull();
    }

    @Test
    void chiudiBando() {
        // TODO arrange, act, assert
    }
}
`;

const RUST_TAURI = `#[tauri::command]
pub async fn load_project(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    path: String,
    recursive: bool,
) -> Result<ProjectInfo, String> {
    todo!()
}
`;

const TS_TAURI = `export function loadProject(path: string, recursive: boolean): Promise<ProjectInfo> {
  return invoke<ProjectInfo>('load_project', { path, recursive });
}
`;

const RUST_BEVY = `fn move_players(time: Res<Time>, mut query: Query<(&mut Transform, &Player), Without<Frozen>>) {
    for (mut transform, player) in &mut query {
        transform.translation.x += player.speed * time.delta_secs();
    }
}
`;

const file = (name: string, text: string, language = 'java', dir = MAIN) => ({ path: `${dir}/${name}`, name, text, language, exists: false });
const empty: GeneratorPreview = { files: [], edits: [], members: null, wiring: [], extras: [], notes: [], dependencies: [] };
const members = (text: string, cls = 'Bando'): GeneratorPreview['members'] => ({ text, file: `${MAIN}/model/${cls}.java`, class: cls });

const PREVIEWS: Record<string, (o: Record<string, any>) => GeneratorPreview> = {
  'spring-security': (o) => ({
    ...empty,
    files: [file(`${o.class_name || 'SecurityConfig'}.java`, JAVA_SECURITY, 'java', `${DEMO}/web/src/main/java/it/comune/gestionale/web/config`)],
    wiring: ['application.yml: spring.security.oauth2.resourceserver.jwt.issuer-uri'],
    dependencies: ['org.springframework.boot:spring-boot-starter-oauth2-resource-server'],
    notes: ['Method security is on: put @PreAuthorize on the service methods that need it.'],
  }),
  'attribute-converter': (o) => ({
    ...empty,
    files: [file(`${o.class_name || 'LocalDateStringConverter'}.java`, JAVA_CONVERTER, 'java', `${MAIN}/model/converter`)],
    wiring: ['Bando.scadenza: @Convert(converter = LocalDateStringConverter.class)'],
    edits: [{ start: 310, end: 310, replacement: '@Convert(converter = LocalDateStringConverter.class)\n' }],
    notes: ['autoApply is on: every LocalDate attribute of every entity uses this converter.'],
  }),
  'jackson-serde': (o) => ({
    ...empty,
    files: [file(`${o.class_name || 'BandoJson'}.java`, JAVA_JACKSON, 'java', `${MAIN}/model/json`)],
    notes: ['Property names are written in snake_case; the deserializer accepts the same.'],
  }),
  'http-client': (o) => ({
    ...empty,
    files: [
      file(`${o.class_name || 'BandoClient'}.java`, JAVA_CLIENT, 'java', `${DEMO}/client/src/main/java/it/comune/gestionale/client`),
      file('BandoClientException.java', 'package it.comune.gestionale.client;\n\npublic class BandoClientException extends RuntimeException {\n    private final int status;\n\n    public BandoClientException(int status, java.io.InputStream body) {\n        super("HTTP " + status);\n        this.status = status;\n    }\n\n    public int status() { return status; }\n}\n', 'java', `${DEMO}/client/src/main/java/it/comune/gestionale/client`),
    ],
    extras: [{ label: 'application.yml', language: 'yaml', text: 'gestionale:\n  bandi:\n    base-url: http://localhost:8080\n' }],
    dependencies: ['org.springframework:spring-web'],
  }),
  'dto-mapper': () => ({
    ...empty,
    files: [file('BandoDto.java', JAVA_DTO, 'java', `${MAIN}/model/dto`), file('BandoMapper.java', JAVA_MAPPER, 'java', `${MAIN}/model/dto`)],
    dependencies: ['org.mapstruct:mapstruct-processor'],
  }),
  builder: () => ({
    ...empty,
    edits: [{ start: 800, end: 800, replacement: JAVA_BUILDER }],
    members: members(JAVA_BUILDER),
    wiring: ['import java.util.Objects'],
  }),
  'value-members': () => ({
    ...empty,
    edits: [{ start: 800, end: 800, replacement: JAVA_VALUE }],
    members: members(JAVA_VALUE),
    notes: ['Entity: equals compares the id only, so a detached copy equals the managed one.'],
  }),
  'test-skeleton': () => ({
    ...empty,
    files: [file('BandoServiceImplTest.java', JAVA_TEST, 'java', `${DEMO}/core/src/test/java/it/comune/gestionale/core/service`)],
  }),
  'tauri-command': () => ({
    ...empty,
    edits: [{ start: 1200, end: 1200, replacement: RUST_TAURI }],
    members: { text: RUST_TAURI, file: 'src-tauri/src/commands/project.rs', class: null },
    wiring: ['lib.rs: generate_handler![… commands::project::load_project]'],
    extras: [{ label: 'project.ts', language: 'typescript', text: TS_TAURI }],
  }),
  'bevy-skeleton': () => ({
    ...empty,
    edits: [{ start: 400, end: 400, replacement: RUST_BEVY }],
    members: { text: RUST_BEVY, file: 'src/systems.rs', class: null },
    wiring: ['App::add_systems(Update, move_players)'],
  }),
};

const argsOf = (p: Record<string, any>): Record<string, any> => (p.args ?? p) as Record<string, any>;

export const genFixtures: Record<string, FixtureHandler> = {
  'bennu.bennu_generate_open': (p) => OPENED[argsOf(p).id] ?? null,
  'bennu.bennu_generate_preview': (p) => {
    const a = argsOf(p);
    return PREVIEWS[a.id]?.(a.options ?? {}) ?? empty;
  },
  'bennu.bennu_generate_list': [],
};
