package {{package}};

import java.io.IOException;

import {{ns}}.servlet.annotation.WebServlet;
import {{ns}}.servlet.http.HttpServlet;
import {{ns}}.servlet.http.HttpServletRequest;
import {{ns}}.servlet.http.HttpServletResponse;

@WebServlet("/hello")
public class HelloServlet extends HttpServlet {

    @Override
    protected void doGet(HttpServletRequest req, HttpServletResponse resp) throws IOException {
        String name = req.getParameter("name");
        boolean anonymous = name == null || name.trim().isEmpty();
        resp.setContentType("text/plain;charset=UTF-8");
        resp.getWriter().println("Hello, " + (anonymous ? "world" : name) + "!");
    }
}
