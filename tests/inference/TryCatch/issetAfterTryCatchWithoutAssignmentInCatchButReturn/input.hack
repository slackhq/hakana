function test(): string {
    throw new Exception("bad");
}

$a = "foo";

try {
    $var = test();
} catch (Exception $e) {
    return;
}

echo $var;

echo $a;