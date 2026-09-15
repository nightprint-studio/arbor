package corpus.legacy.service;

import static corpus.legacy.util.DateUtil.*;
import static corpus.legacy.util.StringUtil.default_if_blank;
import static corpus.legacy.util.StringUtil.is_blank;

import java.sql.SQLException;
import java.util.*;

import corpus.legacy.dao.UserDao;
import corpus.legacy.dao.UserRecord;
import corpus.legacy.util.StringUtil;

/** Service over the DAO: static imports (single and on-demand), java.util on demand, wrapped SQLExceptions. */
public class UserService {

    private final UserDao user_dao;
    private final Map<String, List<String>> email_cache = new HashMap<String, List<String>>();

    public UserService(UserDao user_dao) {
        this.user_dao = user_dao;
    }

    public List<String> active_emails(String role) throws ServiceException {
        if (is_blank(role)) {
            return Collections.emptyList();
        }
        List<String> cached = email_cache.get(role);
        if (cached != null) {
            return cached;
        }
        try {
            List<String> emails = new ArrayList<String>();
            for (UserRecord record : user_dao.find_by_role(role)) {
                if (!is_blank(record.getEmail()) && is_recent(record.getCreated_at())) {
                    emails.add(record.getEmail().toLowerCase());
                }
            }
            email_cache.put(role, emails);
            return emails;
        } catch (SQLException e) {
            throw new ServiceException("lookup failed for role " + role, e);
        }
    }

    public String describe(UserRecord record) {
        Date created = record.getCreated_at();
        String since = created == null ? "unknown" : String.valueOf(start_of_day(created).getTime() / DAY_MILLIS);
        return default_if_blank(record.getUser_name(), StringUtil.EMPTY) + " since day " + since;
    }

    public int change_email(long id, String email) throws ServiceException {
        if (is_blank(email)) {
            throw new ServiceException("blank email for user " + id, null);
        }
        try {
            email_cache.clear();
            return user_dao.update_email(id, email.trim());
        } catch (SQLException e) {
            throw new ServiceException("update failed for user " + id, e);
        }
    }
}
