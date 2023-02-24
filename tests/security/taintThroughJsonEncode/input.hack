$a = dict['foo' => $_GET['foo']];
$b = dict['a' => $a];
$c = dict['b' => $b];

echo foo($c);

function foo(dict<string, mixed> $c): string {
    $d = json_encode($c);
    if ($d is string) {
        return $d;
    }
    return '';
}