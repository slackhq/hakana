use type Facebook\XHP\HTML\button;
$url = (string)HH\global_get('_GET')['url'];
$element = <button formaction={$url} />;
