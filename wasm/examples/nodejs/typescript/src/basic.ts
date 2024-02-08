import {version} from "../../kaspa/kaspa_wasm";

console.log("version", version());

/**
 * @public
 * StatisticsBasic2 class (basic file)
 */
export class StatisticsBasic2 {
    /**
     * Returns the average of two numbers.
     *
     * @remarks
     * This method is part of the {@link StatisticsBasic2 | Statistics subsystem}.
     *
     * @param x - The first input number
     * @param y - The second input number
     * @returns The arithmetic mean of `x` and `y`
     *
     * @beta
     */
    public static getAverage(x: number, y: number): number {
      return (x + y) / 2.0;
    }
}

export {version};
