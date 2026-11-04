//! A hand-built context for the generators' tests.

use std::collections::BTreeMap;

use crate::context::*;

pub fn project() -> ProjectInfo {
    ProjectInfo::new(
        17,
        &[
            ("org.springframework.boot:spring-boot", "3.2.1"),
            ("org.springframework.security:spring-security-web", "6.2.1"),
            ("org.springframework.boot:spring-boot-starter-security", "3.2.1"),
            ("org.springframework.boot:spring-boot-starter-oauth2-resource-server", "3.2.1"),
            ("org.springframework.boot:spring-boot-starter-web", "3.2.1"),
            ("org.springframework:spring-web", "6.1.2"),
            ("com.fasterxml.jackson.core:jackson-databind", "2.15.3"),
            ("jakarta.persistence:jakarta.persistence-api", "3.1.0"),
        ],
    )
}

pub fn legacy_project() -> ProjectInfo {
    ProjectInfo::new(
        8,
        &[
            ("org.springframework.boot:spring-boot", "2.7.18"),
            ("org.springframework.security:spring-security-web", "5.7.11"),
            ("javax.persistence:javax.persistence-api", "2.2"),
        ],
    )
}

pub fn customer() -> ClassInfo {
    let mut id = FieldInfo::new("id", "Long");
    id.id = true;
    ClassInfo {
        name: "Customer".into(),
        package: "com.acme.shop.model".into(),
        kind: "class".into(),
        annotations: vec!["Entity".into()],
        fields: vec![
            id,
            FieldInfo::new("name", "String"),
            FieldInfo::new("email", "String"),
            FieldInfo::new("birthDate", "LocalDate"),
            FieldInfo::new("tags", "List<String>"),
        ],
        ..Default::default()
    }
}

pub fn endpoints() -> Vec<EndpointInfo> {
    let p = |name: &str, ty: &str, binding: &str| EndpointParam { name: name.into(), type_text: ty.into(), binding: binding.into(), required: true };
    vec![
        EndpointInfo {
            id: "GET /api/orders/{id}".into(),
            methods: vec!["GET".into()],
            path: "/api/orders/{id}".into(),
            controller: "OrderController".into(),
            handler: "get".into(),
            return_type: "OrderDto".into(),
            produces: String::new(),
            params: vec![p("id", "Long", "path"), EndpointParam { required: false, ..p("expand", "String", "query") }],
        },
        EndpointInfo {
            id: "POST /api/orders".into(),
            methods: vec!["POST".into()],
            path: "/api/orders".into(),
            controller: "OrderController".into(),
            handler: "create".into(),
            return_type: "ResponseEntity<OrderDto>".into(),
            produces: String::new(),
            params: vec![p("request", "CreateOrderRequest", "body"), p("X-Tenant", "String", "header")],
        },
        EndpointInfo {
            id: "GET /api/orders".into(),
            methods: vec!["GET".into()],
            path: "/api/orders".into(),
            controller: "OrderController".into(),
            handler: "list".into(),
            return_type: "List<OrderDto>".into(),
            produces: String::new(),
            params: vec![p("status", "String", "query"), p("page", "int", "query")],
        },
        EndpointInfo {
            id: "DELETE /api/orders/{id}".into(),
            methods: vec!["DELETE".into()],
            path: "/api/orders/{id}".into(),
            controller: "OrderController".into(),
            handler: "delete".into(),
            return_type: "void".into(),
            produces: String::new(),
            params: vec![p("id", "Long", "path")],
        },
    ]
}

pub fn sample_context() -> GenContext {
    let mut types = BTreeMap::new();
    types.insert(
        "OrderDto".to_string(),
        ClassInfo::new("OrderDto", "com.acme.shop.api", vec![FieldInfo::new("id", "Long"), FieldInfo::new("total", "BigDecimal")]),
    );
    types.insert(
        "CreateOrderRequest".to_string(),
        ClassInfo::new("CreateOrderRequest", "com.acme.shop.api", vec![FieldInfo::new("customerId", "Long"), FieldInfo::new("lines", "List<String>")]),
    );
    GenContext {
        project: project(),
        class: Some(customer()),
        file: Some("/p/src/main/java/com/acme/shop/model/Customer.java".into()),
        package: "com.acme.shop".into(),
        endpoints: endpoints(),
        types,
        app_name: "shop".into(),
    }
}
