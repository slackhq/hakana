use type Facebook\XHP\HTML\script;
$url = (string)HH\global_get('_GET')['url'];
$script = <script src={'https://example.com/uploads/'.rawurlencode($url)} />;
