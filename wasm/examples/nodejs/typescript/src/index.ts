import {version} from "./basic";
import * as types from "../../kaspa/kaspa_wasm.d";

import {Statistics} from "./address";

console.log("API: version", version());
console.log("Statistics", Statistics);

export * from "../../kaspa/kaspa_wasm";

export {Statistics};

/**
 * @public
 * StatisticsBasic class (index file)
 */
export class StatisticsBasic111 {
    /**
     * Returns the average of two numbers.
     *
     * @remarks
     * This method is part of the {@link StatisticsBasic111 | Statistics subsystem}.
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
