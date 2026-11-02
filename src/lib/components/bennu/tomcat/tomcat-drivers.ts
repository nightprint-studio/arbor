/**
 * The JDBC drivers the form offers to fill in — a driver class and the skeleton of its URL.
 *
 * A list the person picks from rather than a catalogue the backend answers, because the answer
 * never changes between launches and typing `oracle.jdbc.OracleDriver` from memory is the part of
 * writing a context nobody enjoys. It is deliberately the same set `bennu-tomcat::drivers` knows how
 * to find a jar for: offering a driver the launcher cannot place would be a preset that fails later.
 */

export interface DriverPreset {
  label: string;
  /** The driver class. */
  driver: string;
  /** A URL to complete — used only when the field is empty. */
  urlTemplate: string;
}

export const DRIVER_PRESETS: DriverPreset[] = [
  { label: 'PostgreSQL', driver: 'org.postgresql.Driver', urlTemplate: 'jdbc:postgresql://localhost:5432/database' },
  { label: 'MySQL', driver: 'com.mysql.cj.jdbc.Driver', urlTemplate: 'jdbc:mysql://localhost:3306/database' },
  { label: 'MariaDB', driver: 'org.mariadb.jdbc.Driver', urlTemplate: 'jdbc:mariadb://localhost:3306/database' },
  { label: 'Oracle', driver: 'oracle.jdbc.OracleDriver', urlTemplate: 'jdbc:oracle:thin:@//localhost:1521/service' },
  { label: 'IBM Db2', driver: 'com.ibm.db2.jcc.DB2Driver', urlTemplate: 'jdbc:db2://localhost:50000/database' },
  { label: 'SQL Server', driver: 'com.microsoft.sqlserver.jdbc.SQLServerDriver', urlTemplate: 'jdbc:sqlserver://localhost:1433;databaseName=database' },
  { label: 'H2', driver: 'org.h2.Driver', urlTemplate: 'jdbc:h2:mem:test' },
  { label: 'SQLite', driver: 'org.sqlite.JDBC', urlTemplate: 'jdbc:sqlite:file.db' },
];
