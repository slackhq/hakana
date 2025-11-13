$errors = vec[];

try {
    if (rand(0, 1)) {
        throw new Exception("bad");
    }
} catch (Exception $e) {
    $errors[] = $e;
}

if (HH\Lib\C\count($errors) !== 0) {
    echo "Errors";
}