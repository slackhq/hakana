switch (rand(0, 50)) {
    case FORTY: // Observed a valid UndeclaredConstant warning
        $x = "value";
        break;
    default:
        $x = "other";
    }

    echo $x;