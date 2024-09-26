switch (rand(0, 4)) {
    case 0:
        if (rand(0, 1)) {
            $a = 0;
            break;
        }
        // FALLTHROUGH
    default:
        $a = 1;
}

echo $a;