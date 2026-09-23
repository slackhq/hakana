use type Facebook\XHP\HTML\script;

$input = (string)HH\global_get('_GET')['js'];
$first = <script>{$input}</script>;
$second = <script>{$input}</script>;
$safe = <script>{"safe();"}</script>;
