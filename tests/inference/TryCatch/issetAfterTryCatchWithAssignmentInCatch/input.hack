function test(): string {
    throw new Exception("bad");
}

$a = "foo";

try {
    $var = test();
} catch (Exception $e) {
    $var = "bad";
}

echo $var;
echo $a;