#!/usr/bin/env python3
import json
import sys


def load(path):
    with open(path) as f:
        return json.load(f)


def verdict(baseline, candidate):
    total_a = baseline.get('total_score', 0.0)
    total_b = candidate.get('total_score', 0.0)
    hill_a = baseline.get('hillclimb_score', 0.0)
    hill_b = candidate.get('hillclimb_score', 0.0)

    if total_b > total_a:
        return 'improved'
    if total_b < total_a:
        return 'regressed'
    if hill_b > hill_a:
        return 'improved'
    if hill_b < hill_a:
        return 'regressed'
    return 'flat'


def main():
    if len(sys.argv) != 3:
        print('usage: score_compare.py BASELINE_JSON CANDIDATE_JSON', file=sys.stderr)
        sys.exit(2)

    baseline = load(sys.argv[1])
    candidate = load(sys.argv[2])
    decision = verdict(baseline, candidate)

    total_a = baseline.get('total_score', 0.0)
    total_b = candidate.get('total_score', 0.0)
    hill_a = baseline.get('hillclimb_score', 0.0)
    hill_b = candidate.get('hillclimb_score', 0.0)

    print(decision)
    print(f'baseline_total\t{total_a}')
    print(f'candidate_total\t{total_b}')
    print(f'delta_total\t{round(total_b - total_a, 2)}')
    print(f'baseline_hillclimb\t{hill_a}')
    print(f'candidate_hillclimb\t{hill_b}')
    print(f'delta_hillclimb\t{round(hill_b - hill_a, 2)}')


if __name__ == '__main__':
    main()
