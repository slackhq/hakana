$options = getopt("t:");

try {
    if (!isset($options["t"])) {
        throw new Exception("bad");
    }
} catch (Exception $e) {}