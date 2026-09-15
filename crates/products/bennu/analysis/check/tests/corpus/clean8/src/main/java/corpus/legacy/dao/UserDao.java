package corpus.legacy.dao;

import java.sql.Connection;
import java.sql.PreparedStatement;
import java.sql.ResultSet;
import java.sql.SQLException;
import java.sql.Timestamp;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

import javax.sql.DataSource;

/** JDBC through the java.sql interfaces only (no driver): nested try/finally and try-with-resources. */
public class UserDao {

    private static final String SELECT_BY_ROLE =
            "SELECT id, user_name, email, created_at FROM users WHERE role = ?";

    private final DataSource data_source;

    public UserDao(DataSource data_source) {
        this.data_source = data_source;
    }

    public List<UserRecord> find_by_role(String role) throws SQLException {
        Connection connection = data_source.getConnection();
        try {
            PreparedStatement statement = connection.prepareStatement(SELECT_BY_ROLE);
            try {
                statement.setString(1, role);
                ResultSet rs = statement.executeQuery();
                try {
                    List<UserRecord> result = new ArrayList<UserRecord>();
                    while (rs.next()) {
                        result.add(map_row(rs));
                    }
                    return result;
                } finally {
                    rs.close();
                }
            } finally {
                statement.close();
            }
        } finally {
            connection.close();
        }
    }

    public int update_email(long id, String email) throws SQLException {
        try (Connection connection = data_source.getConnection();
             PreparedStatement statement = connection.prepareStatement("UPDATE users SET email = ? WHERE id = ?")) {
            statement.setString(1, email);
            statement.setLong(2, id);
            return statement.executeUpdate();
        }
    }

    public void rename_all(List<UserRecord> records) throws SQLException {
        Connection connection = data_source.getConnection();
        boolean auto_commit = connection.getAutoCommit();
        try {
            connection.setAutoCommit(false);
            PreparedStatement statement = connection.prepareStatement("UPDATE users SET user_name = ? WHERE id = ?");
            try {
                for (Iterator<UserRecord> it = records.iterator(); it.hasNext();) {
                    UserRecord record = it.next();
                    statement.setString(1, record.getUser_name());
                    statement.setLong(2, record.getId());
                    statement.addBatch();
                }
                statement.executeBatch();
                connection.commit();
            } catch (SQLException e) {
                connection.rollback();
                throw e;
            } finally {
                statement.close();
            }
        } finally {
            connection.setAutoCommit(auto_commit);
            connection.close();
        }
    }

    private UserRecord map_row(ResultSet rs) throws SQLException {
        UserRecord record = new UserRecord();
        record.setId(rs.getLong("id"));
        record.setUser_name(rs.getString("user_name"));
        record.setEmail(rs.getString(3));
        Timestamp created = rs.getTimestamp("created_at");
        record.setCreated_at(created == null ? null : new java.util.Date(created.getTime()));
        return record;
    }
}
