<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE struts-config PUBLIC
    "-//Apache Software Foundation//DTD Struts Configuration 1.3//EN"
    "http://struts.apache.org/dtds/struts-config_1_3.dtd">

<struts-config>

    <form-beans>
        <form-bean name="helloForm" type="{{package}}.HelloForm"/>
    </form-beans>

    <action-mappings>
        <action path="/hello"
                type="{{package}}.HelloAction"
                name="helloForm"
                scope="request"
                input="/index.jsp">
            <forward name="success" path="/WEB-INF/jsp/hello.jsp"/>
        </action>
    </action-mappings>

    <message-resources parameter="MessageResources"/>

</struts-config>
