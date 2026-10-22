#ifndef PETRICHOR_PLUGIN_H
#define PETRICHOR_PLUGIN_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * ABI C des plugins natifs Petrichor.
 *
 * Un plugin est une bibliothèque partagée (.so) exportant au minimum :
 *   PluginVersion, Initialize, Update, Finalize.
 * Reload, GetString et ExecuteBang sont optionnels.
 *
 * Les fonctions personnalisées `[MesurePlugin:Nom(...)]` sont résolues à la
 * demande : `const char *Nom(void *data, int argc, const char *argv[])`.
 */

typedef struct PetrichorHostApi {
    void *userdata;

    const char *(*read_string)(void *userdata, const char *option,
                               const char *default_value, int replace);
    double (*read_formula)(void *userdata, const char *option,
                           double default_value);
    void (*execute)(void *userdata, const char *action);
    void (*log)(void *userdata, int level, const char *message);
    const char *(*get)(void *userdata, const char *kind);

    const char *(*read_string_from_section)(void *userdata, const char *section,
                                            const char *option,
                                            const char *default_value,
                                            int replace);
    double (*read_formula_from_section)(void *userdata, const char *section,
                                        const char *option,
                                        double default_value);
    int (*read_int)(void *userdata, const char *option, int default_value);
    double (*read_double)(void *userdata, const char *option,
                          double default_value);
    const char *(*read_path)(void *userdata, const char *option,
                             const char *default_value);
    const char *(*replace_variables)(void *userdata, const char *text);
    const char *(*path_to_absolute)(void *userdata, const char *path);
} PetrichorHostApi;

#define PETRICHOR_PLUGIN_VERSION 4u

/* Niveaux de journalisation. */
#define PETRICHOR_LOG_ERROR 1
#define PETRICHOR_LOG_WARNING 2
#define PETRICHOR_LOG_NOTICE 3
#define PETRICHOR_LOG_DEBUG 4

/* Symboles obligatoires. */
uint32_t PluginVersion(void);
void Initialize(void **data, PetrichorHostApi *api);
double Update(void *data);
void Finalize(void *data);

/* Symboles optionnels. */
void Reload(void *data, PetrichorHostApi *api, double *max_value);
const char *GetString(void *data);
void ExecuteBang(void *data, const char *args);
const char *PluginAuthor(void);

#ifdef __cplusplus
}
#endif

#endif
