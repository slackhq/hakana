$a = HH\global_get('_GET')["name"];
$b = explode(" ", $a);
$c = implode(" ", $b);
echo $c;