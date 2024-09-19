$a = vec[];
$a[] = dict["a" => HH\global_get('_GET')["name"], "b" => "foo"];

foreach ($a as $m) {
    echo $m["b"];
}