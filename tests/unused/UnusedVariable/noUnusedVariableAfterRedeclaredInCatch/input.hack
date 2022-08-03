$path = "";

echo $path;

try {
    // do nothing
} catch (\Exception $exception) {
    $path = "hello";
}

echo $path;