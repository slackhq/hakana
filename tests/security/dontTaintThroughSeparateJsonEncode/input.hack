$a = $_GET["bad"];

$b = json_encode($a);

function foo(mixed $c) {
    $d = json_encode($c);
    echo $d;
}
