package corpus.reported;

/** Legal twin of {@link ReportedRecordBad}: the same method against a client that takes the {@code Delegate}. */
public class ReportedRecordOk {
    private final ReportedRecordTypes.IdentityResolver resolver;
    private final ReportedRecordTypes.DelegateClient client;

    ReportedRecordOk(ReportedRecordTypes.IdentityResolver resolver, ReportedRecordTypes.DelegateClient client) {
        this.resolver = resolver;
        this.client = client;
    }

    boolean checkDelegate(final String username) {
        var delegateOpt = resolver.resolveIdentity();

        if (delegateOpt.isEmpty())
            return false;

        var delegate = delegateOpt.get();

        var delegateDbOpt =
            client.delegateForUser(
                username
                , delegate.identifier()
                , delegateOpt.get()
            );

        return delegateDbOpt.isPresent();
    }
}
