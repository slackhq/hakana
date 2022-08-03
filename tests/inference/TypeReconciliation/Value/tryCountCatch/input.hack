$errors = vec[];

try {
    if (rand(0, 1)) {
        throw new Exception("bad");
    }
} catch (Exception $e) {
    $errors[] = $e;
}

if (count($errors) !== 0) {
    echo "Errors";
}