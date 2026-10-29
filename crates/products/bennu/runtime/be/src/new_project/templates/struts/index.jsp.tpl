<%@ page contentType="text/html;charset=UTF-8" language="java" %>
<%@ taglib uri="http://struts.apache.org/tags-html" prefix="html" %>
<%@ taglib uri="http://struts.apache.org/tags-bean" prefix="bean" %>
<html:html>
<head>
    <title><bean:message key="app.title"/></title>
</head>
<body>
    <h1><bean:message key="app.title"/></h1>
    <html:form action="/hello">
        <bean:message key="hello.name"/>
        <html:text property="name"/>
        <html:submit><bean:message key="hello.submit"/></html:submit>
    </html:form>
</body>
</html:html>
