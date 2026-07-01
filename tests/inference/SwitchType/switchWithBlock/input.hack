function foo(): void {
    $letter = rand(0, 1) !== 0 ? 'a' : (rand(0, 1) !== 0 ? 'b' : (rand(0, 1) !== 0 ? 'c' : 'd'));
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
