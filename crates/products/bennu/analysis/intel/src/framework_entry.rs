//! **Who calls this, if no code does** — the annotations that mean a framework invokes a
//! declaration itself.
//!
//! ## Why a list, and why this one
//!
//! The usage counts start from the reference index, which sees calls written in code. Plenty of
//! Java is not called from code at all: a `@Test` is invoked by JUnit, a `@Bean` factory by the
//! Spring container, a `@PrePersist` by the JPA provider, a `@GetMapping` by the servlet
//! dispatcher. Every one of those has a genuine count of zero, and reporting it is the editor
//! saying something true and useless — "no usages" on every test method in the project.
//!
//! So this is the list of **entry points**: places where a count of zero is not information, it is
//! the wrong question. A mark for one of these is not drawn at all rather than drawn with an
//! explanation attached, which is the difference between an editor that annotates your test file
//! and one that clutters it.
//!
//! ## Not the same list as "might be reached"
//!
//! There is a second, deliberately broader rule elsewhere: **any** annotation means a framework
//! *might* reach a member by name, which is what [`crate::safe_delete`] refuses on and what stops
//! the usage counts greying anything annotated. That one is a refusal to claim knowledge. This one
//! is knowledge — each entry names a framework and what it does — and it is what earns the right
//! to say nothing at all.
//!
//! An annotation not on this list falls through to the broader rule: the count is shown, and the
//! name is not dimmed. Being missing from here costs a row nobody needed; being wrongly on it
//! would hide a count somebody wanted. That asymmetry is why the list is written out by hand
//! rather than guessed at from a package prefix.
//!
//! ## Test annotations are not repeated here
//!
//! They live in [`bennu_test`], which already had them for test discovery, and are read from
//! there. Two lists of "what JUnit calls" would drift into disagreeing about which methods a test
//! framework invokes.

/// Annotations on a **method** that mean something outside the code calls it.
///
/// Grouped by what does the calling, because that is the only way to check the list is right.
const METHOD_ENTRY: &[(&str, &str)] = &[
    // Spring — the container builds beans and runs lifecycle callbacks.
    ("Bean", "Spring builds it"),
    ("PostConstruct", "the container calls it after injection"),
    ("PreDestroy", "the container calls it on shutdown"),
    ("EventListener", "Spring delivers events to it"),
    ("Scheduled", "the scheduler runs it"),
    ("TransactionalEventListener", "Spring delivers events to it"),
    // Setter injection: the container calls the method to hand it a dependency. `@Value` on a
    // method is the same gesture with a property instead of a bean.
    ("Autowired", "the container calls it to inject"),
    ("Required", "the container calls it to inject"),
    // Spring Test contributes properties by calling a static method it finds by annotation —
    // nothing in the codebase names it, which is the whole reason it is written this way.
    ("DynamicPropertySource", "Spring Test calls it for the property source"),
    ("BeforeTransaction", "Spring Test calls it around the transaction"),
    ("AfterTransaction", "Spring Test calls it around the transaction"),
    // Spring Retry: the recovery method is invoked by the interceptor, never called.
    ("Recover", "Spring Retry calls it when the retries are exhausted"),
    // EJB / JAX-WS: the container is the only caller.
    ("Schedule", "the EJB timer service calls it"),
    ("Timeout", "the EJB timer service calls it"),
    ("WebMethod", "the web-service runtime calls it"),
    // Spring MVC / WebFlux — the dispatcher invokes handlers by mapping, never by name.
    ("RequestMapping", "the request dispatcher calls it"),
    ("GetMapping", "the request dispatcher calls it"),
    ("PostMapping", "the request dispatcher calls it"),
    ("PutMapping", "the request dispatcher calls it"),
    ("DeleteMapping", "the request dispatcher calls it"),
    ("PatchMapping", "the request dispatcher calls it"),
    ("ExceptionHandler", "the dispatcher calls it on a failure"),
    ("ModelAttribute", "the dispatcher calls it before a handler"),
    ("InitBinder", "the dispatcher calls it before binding"),
    ("MessageMapping", "the message broker calls it"),
    // JAX-RS — the same idea in the other web stack.
    ("GET", "the JAX-RS runtime calls it"),
    ("POST", "the JAX-RS runtime calls it"),
    ("PUT", "the JAX-RS runtime calls it"),
    ("DELETE", "the JAX-RS runtime calls it"),
    ("HEAD", "the JAX-RS runtime calls it"),
    ("OPTIONS", "the JAX-RS runtime calls it"),
    // Messaging — a listener is invoked by the broker's client, from its own threads.
    ("KafkaListener", "the Kafka client calls it"),
    ("RabbitListener", "the AMQP client calls it"),
    ("JmsListener", "the JMS client calls it"),
    ("SqsListener", "the SQS client calls it"),
    ("StreamListener", "the stream binder calls it"),
    // JPA / Hibernate entity lifecycle callbacks.
    ("PrePersist", "the persistence provider calls it"),
    ("PostPersist", "the persistence provider calls it"),
    ("PreUpdate", "the persistence provider calls it"),
    ("PostUpdate", "the persistence provider calls it"),
    ("PreRemove", "the persistence provider calls it"),
    ("PostRemove", "the persistence provider calls it"),
    ("PostLoad", "the persistence provider calls it"),
    // AspectJ / Spring AOP advice — woven in, never called.
    ("Before", "the aspect weaver calls it"),
    ("After", "the aspect weaver calls it"),
    ("Around", "the aspect weaver calls it"),
    ("AfterReturning", "the aspect weaver calls it"),
    ("AfterThrowing", "the aspect weaver calls it"),
    ("Pointcut", "it is a pointcut definition"),
    // Jackson — the serializer reaches accessors and creators reflectively.
    ("JsonCreator", "Jackson constructs through it"),
    ("JsonValue", "Jackson serializes through it"),
    ("JsonAnyGetter", "Jackson serializes through it"),
    ("JsonAnySetter", "Jackson deserializes through it"),
    ("JsonGetter", "Jackson serializes through it"),
    ("JsonSetter", "Jackson deserializes through it"),
];

/// Annotations on a **type** that mean a framework instantiates or dispatches to it, rather than
/// any code writing `new`.
const TYPE_ENTRY: &[(&str, &str)] = &[
    // Spring stereotypes — component scanning finds these, nothing constructs them.
    ("Component", "component scanning finds it"),
    ("Service", "component scanning finds it"),
    ("Repository", "component scanning finds it"),
    ("Controller", "component scanning finds it"),
    ("RestController", "component scanning finds it"),
    ("Configuration", "component scanning finds it"),
    ("ControllerAdvice", "component scanning finds it"),
    ("RestControllerAdvice", "component scanning finds it"),
    ("SpringBootApplication", "it is the application entry point"),
    ("Aspect", "the aspect weaver applies it"),
    ("ConfigurationProperties", "the container binds properties into it"),
    ("JsonComponent", "component scanning finds it"),
    // Spring Data / Cloud: the implementation is generated, so nothing in the codebase declares
    // one and the interface is reached only through the container.
    ("FeignClient", "Spring Cloud implements it"),
    ("RepositoryRestResource", "Spring Data exposes it"),
    // MyBatis maps an interface onto statements and supplies the implementation.
    ("Mapper", "MyBatis implements it"),
    // Spring Test slices: the runner builds the class and drives it.
    ("SpringBootTest", "the test runner builds it"),
    ("DataJpaTest", "the test runner builds it"),
    ("WebMvcTest", "the test runner builds it"),
    ("DataJdbcTest", "the test runner builds it"),
    ("JdbcTest", "the test runner builds it"),
    ("JsonTest", "the test runner builds it"),
    ("RestClientTest", "the test runner builds it"),
    ("TestConfiguration", "the test runner builds it"),
    ("Testcontainers", "the test runner builds it"),
    // How a test class says which engine runs it — JUnit 5 and JUnit 4 respectively. A class
    // carrying either is a class something else instantiates.
    ("ExtendWith", "the test runner builds it"),
    ("RunWith", "the test runner builds it"),
    ("Nested", "the test runner builds it"),
    // CDI and EJB scopes: the container manages the lifecycle.
    ("ApplicationScoped", "the CDI container manages it"),
    ("RequestScoped", "the CDI container manages it"),
    ("SessionScoped", "the CDI container manages it"),
    ("Singleton", "the container manages it"),
    ("Stateless", "the EJB container manages it"),
    ("Stateful", "the EJB container manages it"),
    ("MessageDriven", "the EJB container delivers messages to it"),
    ("WebService", "the web-service runtime publishes it"),
    // JPA — the provider maps and instantiates these.
    ("Entity", "the persistence provider maps it"),
    ("Embeddable", "the persistence provider maps it"),
    ("MappedSuperclass", "the persistence provider maps it"),
    ("Converter", "the persistence provider applies it"),
    // Servlet 3 declarative registration — the container instantiates these from the annotation.
    ("WebServlet", "the servlet container instantiates it"),
    ("WebFilter", "the servlet container instantiates it"),
    ("WebListener", "the servlet container instantiates it"),
    // JAX-RS resources and providers.
    ("Path", "the JAX-RS runtime dispatches to it"),
    ("Provider", "the JAX-RS runtime registers it"),
];

/// Why a framework calls the **method** carrying `annotation`, or `None` when none does.
///
/// Test annotations are answered from [`bennu_test`]'s own tables rather than repeated here.
pub fn entry_for_method(annotation: &str) -> Option<&'static str> {
    if annotation == "Test" {
        return Some("the test runner runs it");
    }
    if bennu_test::prelude::DYNAMIC_TEST_ANNOTATIONS.contains(&annotation) {
        return Some("the test runner runs it");
    }
    if bennu_test::prelude::LIFECYCLE_ANNOTATIONS.contains(&annotation) {
        return Some("the test runner calls it around the tests");
    }
    METHOD_ENTRY.iter().find(|(name, _)| *name == annotation).map(|(_, why)| *why)
}

/// Why a framework reaches the **type** carrying `annotation`, or `None` when none does.
pub fn entry_for_type(annotation: &str) -> Option<&'static str> {
    // A class-level `@Test` is TestNG's way of saying every method in it is a test.
    if annotation == "Test" {
        return Some("the test runner runs it");
    }
    TYPE_ENTRY.iter().find(|(name, _)| *name == annotation).map(|(_, why)| *why)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frameworks_that_call_a_method_are_recognised() {
        for a in ["Test", "BeforeEach", "AfterEach", "ParameterizedTest", "BeforeMethod"] {
            assert!(entry_for_method(a).is_some(), "@{a} is invoked by a test runner");
        }
        for a in ["Bean", "PostConstruct", "GetMapping", "PrePersist", "KafkaListener", "Around"] {
            assert!(entry_for_method(a).is_some(), "@{a} is invoked by a framework");
        }
        // The one that produced the report: nothing in a codebase ever names a
        // `@DynamicPropertySource` method — that is why it is written as an annotation.
        for a in ["DynamicPropertySource", "BeforeTransaction", "Recover", "Autowired"] {
            assert!(entry_for_method(a).is_some(), "@{a} is invoked by a framework");
        }
    }

    #[test]
    fn an_annotation_that_only_describes_a_method_is_not_an_entry_point() {
        // These say something ABOUT the method. Nothing calls it because of them, so a count of
        // zero on one of them is a real finding and must still be shown.
        // `@Transactional` and `@PreAuthorize` belong here and not above: they **wrap** a method
        // that something else calls. Wrapping is not calling, and treating it as an entry point
        // would silence the count on half a service layer.
        for a in ["Override", "Deprecated", "SuppressWarnings", "Nullable", "Transactional", "PreAuthorize"] {
            assert_eq!(entry_for_method(a), None, "@{a} does not make anything call it");
        }
    }

    #[test]
    fn the_frameworks_that_instantiate_a_type_are_recognised() {
        for a in ["Service", "RestController", "Configuration", "Entity", "WebServlet", "Aspect"] {
            assert!(entry_for_type(a).is_some(), "@{a} is built by a framework");
        }
        // A test class says which engine runs it; a Spring Data / MyBatis interface has no
        // implementation in the codebase at all.
        for a in ["SpringBootTest", "ExtendWith", "RunWith", "Nested", "Mapper", "FeignClient"] {
            assert!(entry_for_type(a).is_some(), "@{a} is built by a framework");
        }
    }

    #[test]
    fn a_type_annotation_that_only_describes_a_class_is_not_an_entry_point() {
        for a in ["Data", "Builder", "Slf4j", "Deprecated", "FunctionalInterface"] {
            assert_eq!(entry_for_type(a), None, "@{a} does not make anything build it");
        }
    }

    #[test]
    fn a_method_entry_point_is_not_automatically_a_type_one() {
        // `@Bean` on a class means nothing; the mistake would be a single shared list.
        assert!(entry_for_method("Bean").is_some());
        assert_eq!(entry_for_type("Bean"), None);
        assert!(entry_for_type("Entity").is_some());
        assert_eq!(entry_for_method("Entity"), None);
    }

    #[test]
    fn a_class_level_test_marks_the_class_and_its_methods() {
        // TestNG puts `@Test` on the class to mean "all of these".
        assert!(entry_for_type("Test").is_some());
        assert!(entry_for_method("Test").is_some());
    }

    #[test]
    fn every_reason_reads_as_a_sentence_about_who_calls_it() {
        for (name, why) in METHOD_ENTRY.iter().chain(TYPE_ENTRY) {
            assert!(!why.is_empty(), "@{name} has no reason");
        }
    }
}
