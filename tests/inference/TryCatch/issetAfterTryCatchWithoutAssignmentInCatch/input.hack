function test(): string {
    throw new Exception("bad");
}

$a = "foo";

try {
    $var = test();
} catch (Exception $e) {
    echo "bad";
}

if (isset($var)) {}

echo $a;