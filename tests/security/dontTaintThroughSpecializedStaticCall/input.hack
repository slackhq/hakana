$a = $_GET["bad"];

$b = A::reflect($a);

function foo(mixed $c) {
    $d = A::reflect($c);
    echo $d;
}

class A {
    <<Hakana\SecurityAnalysis\SpecializeCall()>>
    public static function reflect(string $s): string {
        return $s;
    }
}