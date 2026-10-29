<%@ page contentType="text/html;charset=UTF-8" language="java" %>
<%@ taglib prefix="c" uri="{{jstl_uri}}" %>
<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>{{title}}</title>
</head>
<body>
    <h1>Hello from {{title}}</h1>
    <p>Context path: <c:out value="${pageContext.request.contextPath}"/></p>
    <p><a href="hello?name=World">Call the servlet</a></p>
</body>
</html>
