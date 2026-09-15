package corpus.reported;

/**
 * The reported method with {@code var} standing in for Lombok's {@code val}, formatted as reported:
 * the third argument is a {@code Delegate} where a {@code Token} is expected. Twin: {@link ReportedRecordOk}.
 */
public class ReportedRecordBad {
    private final ReportedRecordTypes.IdentityResolver resolver;
    private final ReportedRecordTypes.TokenClient client;

    ReportedRecordBad(ReportedRecordTypes.IdentityResolver resolver, ReportedRecordTypes.TokenClient client) {
        this.resolver = resolver;
        this.client = client;
    }

    boolean checkDelegate(final String username) {
        var delegateOpt = resolver.resolveIdentity();

        if (delegateOpt.isEmpty())
            return false;

        var delegate = delegateOpt.get();

        var delegateDbOpt =
            client.delegateForUser( // error: compiler.err.cant.apply.symbol
                username
                , delegate.identifier()
                , delegateOpt.get()
            );

        return delegateDbOpt.isPresent();
    }
}
