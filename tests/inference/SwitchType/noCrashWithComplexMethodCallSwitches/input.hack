function fromFoo(): int {
   switch (true) {
       case (rand(0, 1) !== 0 && rand(0, 2) !== 0):
       case (rand(0, 3) !== 0 && rand(0, 4) !== 0):
       case (rand(0, 5) !== 0 && rand(0, 6) !== 0):
       case (rand(0, 7) !== 0 && rand(0, 8) !== 0):
       case (rand(0, 7) !== 0 && rand(0, 8) !== 0):
       case (rand(0, 7) !== 0 && rand(0, 8) !== 0):
       case (rand(0, 7) !== 0 && rand(0, 8) !== 0):
           return 1;
       default:
           return 0;
   }
                   }