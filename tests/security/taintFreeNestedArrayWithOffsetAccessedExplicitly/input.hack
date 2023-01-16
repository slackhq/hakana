$a = vec[];
$a[] = dict["a" => $_GET["name"], "b" => "foo"];

echo $a[0]["b"];