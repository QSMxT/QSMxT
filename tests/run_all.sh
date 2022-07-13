set -e

for masking in magnitude-based phase-based bet gaussian-based
do
    python3 ./run_2_qsm.py $@ ../data/qsmxt-test-battery-bids/bids ../data/battery_$masking --masking $masking
done

for masking in grad grad+second grad+second+mag second grad+mag second+mag
do
    for thresh in 0.6
    do
        for hole_filling in 0 1
        do
            python3 ./run_2_qsm.py $@ ../data/qsmxt-test-battery-bids/bids ../data/battery_romeo-phase-based-$masking-$thresh-hole-$hole_filling --masking romeo-phase-based $masking $thresh --extra_fill_strength $hole_filling
        done
    done
done
