package corpus.legacy.service;

/** The service layer's checked exception, wrapping DAO failures. */
public class ServiceException extends Exception {

    private static final long serialVersionUID = 1L;

    public ServiceException(String message, Throwable cause) {
        super(message, cause);
    }
}
