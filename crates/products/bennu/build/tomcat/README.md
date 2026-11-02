# bennu-tomcat

Running a web application on Apache Tomcat, **minus the process and the network**. The backend
starts the JVM and downloads the archive; every decision it needs lives here, where it is tested
without either.

| Module    | What it answers |
|-----------|-----------------|
| `catalog` | The release lines (8.5, 9.0, 10.1, 11.0), the servlet namespace each speaks (`javax` / `jakarta`), where a release is published, how to read the Apache listing and `.sha512` files. |
| `install` | What an unpacked Tomcat is — version from `RELEASE-NOTES`, then its namespace and Java floor. |
| `context` | The application's `context.xml`, generated from a `TomcatRunConfig`: data sources, environment entries, parameters, and — for a live deployment — the resource mounts. |
| `base`    | The generated `CATALINA_BASE` (`server.xml`, the context, `lib/` with the JDBC drivers). Rebuilt every launch; `logs/ temp/ work/` kept. |
| `launch`  | The JVM command line `catalina.bat run` would assemble; secrets as `-D` properties, masked in the display form. |
| `drivers` | Which classpath jar is the driver a data source names, and — for the drivers Maven Central carries — where to fetch it (`central_driver`, `parse_release`). |
| `scan`    | The JNDI names and servlet namespace a project's own files declare. |

Public API: `bennu_tomcat::prelude`.

## Secrets

A data source's password is **never** written to the generated `context.xml` in `secret` mode: the
attribute is `${jdbc.Gestionale.password}`, which Tomcat resolves from a system property, and the
backend passes the property on the command line. The value itself is kept by the backend outside
the repository.
