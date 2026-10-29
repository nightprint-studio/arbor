<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd">
    <modelVersion>4.0.0</modelVersion>

    <groupId>{{group}}</groupId>
    <artifactId>{{artifact}}</artifactId>
    <version>{{version}}</version>
    <packaging>{{packaging}}</packaging>
    <name>{{title}}</name>

    <properties>
        <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
{{#if legacy}}
        <maven.compiler.source>{{java}}</maven.compiler.source>
        <maven.compiler.target>{{java}}</maven.compiler.target>
{{/if}}
{{#unless legacy}}
        <maven.compiler.release>{{java}}</maven.compiler.release>
{{/unless}}
    </properties>
{{#if deps}}

    <dependencies>
{{deps}}
    </dependencies>
{{/if}}
{{#if build}}

    <build>
{{build}}
    </build>
{{/if}}
</project>
