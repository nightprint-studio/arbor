/**
 * The snippet the formatter settings page formats. It is written *unformatted on purpose* — mixed
 * indentation, cramped and loose spacing, braces in both styles, long lines — so that every group
 * of options changes something visible: imports, annotations, Javadoc, a long signature, a
 * stream chain, a ternary, a `&&` condition, a lambda, a switch, an array initialiser.
 */
export const FORMAT_SAMPLE = `package it.comune.gestionale.core.service;
import java.util.List;
import static org.junit.Assert.assertEquals;
import org.springframework.stereotype.Service;
import java.util.stream.Collectors;
import javax.inject.Inject;
import com.acme.model.Bando;
import com.acme.model.Allegato;
import com.acme.model.Stato;
/**
 * Closes the calls that expired.
   * Runs once a night and never touches a call that is still open, whatever its deadline says about it.
 */
@Service @Transactional
public class BandoService
    implements Closeable, Serializable {
  private static final int[] LIMITS = {10,20,30,40,50};
    @Inject private BandoDao dao;
    //keeps the last run


  public List<String> titoliScaduti(Long idUtente, String filtro, boolean soloAperti, int massimo, Comparator<Bando> ordine) throws IOException, SQLException
  {
        if(idUtente==null) return List.of();
    List<String> titoli = dao.findAll().stream().filter(b->b.getScadenza().isBefore(LocalDate.now())).map(Bando::getTitolo).sorted().collect(Collectors.toList());
    String etichetta = soloAperti ? "aperti" : filtro != null ? filtro : "tutti";
    if (idUtente > 0 && massimo > 0 && !titoli.isEmpty() && filtro != null && filtro.length() > 3 || soloAperti)
    {
      log.info("Chiusura di {} bandi per {}", titoli.size(), etichetta);
    }
    else
    {
      return titoli;
    }
    switch(massimo){
      case 0:
        titoli.clear();
        break;
      default:
        titoli = titoli.subList(0,Math.min(massimo,titoli.size()));
    }
    return titoli;
  }
  @Override public void close() { dao.close(); }
}
`;
