$a = vec[];
$a[] = dict["a" => HH\global_get('_GET')["name"], "b" => "foo"];

echo $a[0]["b"];