use type Facebook\XHP\HTML\script;
$html = <script>{htmlspecialchars((string)HH\global_get('_GET')['q'], ENT_QUOTES)}</script>;
