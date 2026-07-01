function foo() : void {
   if (rand(0, 1) !== 0) {
        throw new \Exception("bad");
   }
}

$a = null;

try {
    foo();
    $a = "hello";
} catch (\Exception $e) {
    echo $a;
}

echo $a;
