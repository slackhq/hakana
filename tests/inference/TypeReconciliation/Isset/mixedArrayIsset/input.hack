$get = HH\global_get('_GET') as dict<_, _>;
$a = isset($get["a"]) ? $get["a"] : "";
if ($a) {}