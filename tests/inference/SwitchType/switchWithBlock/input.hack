function foo(): void {
    $letter = rand(0, 1) ? 'a' : (rand(0, 1) ? 'b' : (rand(0, 1) ? 'c' : 'd'));
    switch ($letter) {
    	case 'a':
        	{}
            break;
        case 'b':
        	{}
            break;
        case 'c':
        	{}
            break;
        case 'd':
            break;
    }
}