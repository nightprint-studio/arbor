package corpus.sweep;

/**
 * JasperReports' exporter shape: an interface with two {@code setConfiguration} overloads over two of
 * its type variables, implemented by an abstract class that names the SAME variables in another order.
 * Once the variables are erased the two overloads look identical, but they are different methods.
 */
public class PermutedVariables {
    public interface ReportConfig {
    }

    public interface ExporterConfig {
    }

    public interface PdfReportConfig extends ReportConfig {
    }

    public interface PdfExporterConfig extends ExporterConfig {
    }

    public interface Exporter<I, C extends ReportConfig, E extends ExporterConfig, O> {
        void setConfiguration(C configuration);

        void setConfiguration(E configuration);
    }

    public abstract static class AbstractExporter<RC extends ReportConfig, C extends ExporterConfig, O, X>
            implements Exporter<Object, RC, C, O> {
        private RC itemConfiguration;
        private C exporterConfiguration;

        @Override
        public void setConfiguration(RC configuration) {
            this.itemConfiguration = configuration;
        }

        @Override
        public void setConfiguration(C configuration) {
            this.exporterConfiguration = configuration;
        }
    }

    public static class PdfExporter extends AbstractExporter<PdfReportConfig, PdfExporterConfig, Object, Object> {
    }

    static class SimplePdfReport implements PdfReportConfig {
    }

    static class SimplePdfExporter implements PdfExporterConfig {
    }

    void use(PdfExporter exporter) {
        exporter.setConfiguration(new SimplePdfReport());
        exporter.setConfiguration(new SimplePdfExporter());
    }
}
