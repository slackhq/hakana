use type Facebook\XHP\HTML\form;

$action = (string)HH\global_get('_GET')['action'];
$reviewed = <form action={/* HAKANA_SECURITY_IGNORE[HtmlAttributeUri] */ $action} />;
$unreviewed = <form action={$action} />;
