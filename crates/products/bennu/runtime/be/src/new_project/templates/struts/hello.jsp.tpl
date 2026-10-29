<%@ page contentType="text/html;charset=UTF-8" language="java" %>
<%@ taglib uri="http://struts.apache.org/tags-bean" prefix="bean" %>
<%@ taglib uri="http://struts.apache.org/tags-html" prefix="html" %>
<html:html>
<head>
    <title><bean:message key="app.title"/></title>
</head>
<body>
    <h1><bean:message key="hello.greeting"/> <bean:write name="helloForm" property="name"/>!</h1>
    <p><html:link page="/index.jsp"><bean:message key="hello.back"/></html:link></p>
</body>
</html:html>
