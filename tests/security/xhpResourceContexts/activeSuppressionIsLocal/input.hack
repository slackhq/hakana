use type Facebook\XHP\HTML\script;
$url = (string)HH\global_get('_GET')['url'];
$reviewed = <script src={/* HAKANA_SECURITY_IGNORE[HtmlActiveResourceUri] */ $url} />;
$unreviewed = <script src={$url} />;
