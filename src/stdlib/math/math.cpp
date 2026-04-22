/**
 * sekirei stdlib: math
 */

#include "math.hpp"
#include <cmath>

extern "C" {
    double sk_sqrt(double x)         { return std::sqrt(x); }
    double sk_pow(double x, double y){ return std::pow(x, y); }
    double sk_abs(double x)          { return std::fabs(x); }
    double sk_floor(double x)        { return std::floor(x); }
    double sk_ceil(double x)         { return std::ceil(x); }
    double sk_sin(double x)          { return std::sin(x); }
    double sk_cos(double x)          { return std::cos(x); }
    double sk_log(double x)          { return std::log(x); }
}
