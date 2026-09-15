package corpus.legacy.dao;

import java.io.Serializable;
import java.util.Date;

/** A Struts-era form/transfer bean with snake_case properties. */
public class UserRecord implements Serializable {

    private static final long serialVersionUID = 1L;

    private long id;
    private String user_name;
    private String email;
    private Date created_at;

    public long getId() {
        return id;
    }

    public void setId(long id) {
        this.id = id;
    }

    public String getUser_name() {
        return user_name;
    }

    public void setUser_name(String user_name) {
        this.user_name = user_name;
    }

    public String getEmail() {
        return email;
    }

    public void setEmail(String email) {
        this.email = email;
    }

    public Date getCreated_at() {
        return created_at;
    }

    public void setCreated_at(Date created_at) {
        this.created_at = created_at;
    }
}
