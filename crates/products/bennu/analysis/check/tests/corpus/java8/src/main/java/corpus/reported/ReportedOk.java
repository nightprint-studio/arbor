package corpus.reported;

import java.util.Optional;

import corpus.reported.ReportedTypes.Delegate;
import corpus.reported.ReportedTypes.Token;

/** Legal twin of {@link ReportedBad}: the same calls against a client whose third parameter is a {@link Delegate}. */
public class ReportedOk {
    private final ReportedTypes.IdentityResolver resolver;
    private final ReportedTypes.DelegateClient client;

    ReportedOk(ReportedTypes.IdentityResolver resolver, ReportedTypes.DelegateClient client) {
        this.resolver = resolver;
        this.client = client;
    }

    boolean checkDelegate(final String username) {
        Optional<Delegate> opt = resolver.resolveIdentity();
        if (!opt.isPresent()) {
            return false;
        }
        Delegate d = opt.get();
        Optional<Token> db = client.delegateForUser(username, d.identifier(), opt.get());
        return db.isPresent();
    }

    boolean checkDelegateAsFormatted(final String username) {
        Optional<Delegate> delegateOpt = resolver.resolveIdentity();

        if (!delegateOpt.isPresent())
            return false;

        Delegate delegate = delegateOpt.get();

        Optional<Token> delegateDbOpt =
            client.delegateForUser(
                username
                , delegate.identifier()
                , delegateOpt.get()
            );

        return delegateDbOpt.isPresent();
    }
}
