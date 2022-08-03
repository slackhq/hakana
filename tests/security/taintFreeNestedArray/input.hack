$a = [];
$a[] = ["a" => $_GET["name"], "b" => "foo"];

foreach ($a as $m) {
    echo $m["b"];
}