plugins {
    id 'java'
{{#if sample}}
    id 'application'
{{/if}}
}

group = '{{group}}'
version = '{{version}}'

repositories {
    mavenCentral()
}

java {
    toolchain {
        languageVersion = JavaLanguageVersion.of({{java}})
    }
}
{{#if sample}}

application {
    mainClass = '{{package}}.{{class}}'
}
{{/if}}
{{#if junit}}

dependencies {
    testImplementation 'org.junit.jupiter:junit-jupiter:5.11.3'
    testRuntimeOnly 'org.junit.platform:junit-platform-launcher'
}

tasks.named('test') {
    useJUnitPlatform()
}
{{/if}}
