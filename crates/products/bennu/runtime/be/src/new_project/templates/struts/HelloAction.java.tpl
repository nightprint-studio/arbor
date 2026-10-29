package {{package}};

import javax.servlet.http.HttpServletRequest;
import javax.servlet.http.HttpServletResponse;

import org.apache.struts.action.Action;
import org.apache.struts.action.ActionForm;
import org.apache.struts.action.ActionForward;
import org.apache.struts.action.ActionMapping;

public class HelloAction extends Action {

    @Override
    public ActionForward execute(ActionMapping mapping, ActionForm form,
            HttpServletRequest request, HttpServletResponse response) throws Exception {
        HelloForm helloForm = (HelloForm) form;
        String name = helloForm.getName();
        if (name == null || name.trim().length() == 0) {
            helloForm.setName("world");
        }
        return mapping.findForward("success");
    }
}
