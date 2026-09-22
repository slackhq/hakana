$first = (string)HH\global_get('_GET')['first'];
$second = (string)HH\global_get('_GET')['second'];
echo /* HAKANA_SECURITY_IGNORE[HtmlTag] first output intentionally raw */ $first; echo $second;
